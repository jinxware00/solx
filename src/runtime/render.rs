//! Bounded language formatting. Host Display/Debug retain their compatibility API.
//!
//! Lists and single-property objects append directly to one output buffer, so
//! deep chains do not repeatedly copy every descendant. Objects sort complete
//! rendered entries to preserve the existing unusual-key ordering. Comparison
//! and rearrangement work is charged before it happens; nested branching objects
//! can still take superlinear work, but cannot escape the configured work budget.
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use crate::limits::LimitKind;

use super::{RuntimeError, Value, read_collection};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Id {
    List(usize),
    Object(usize),
}

enum Task {
    Visit(Value),
    List(Rc<RefCell<Vec<Value>>>, usize, usize),
    Object {
        properties: Rc<RefCell<HashMap<String, Value>>>,
        keys: Vec<String>,
        next: usize,
        start: usize,
        entries: Vec<Range<usize>>,
    },
}

struct Budget<F> {
    maximum: usize,
    reserved: usize,
    output: String,
    charge: F,
}

impl<F: FnMut(usize) -> Result<(), RuntimeError>> Budget<F> {
    fn reserve(&mut self, bytes: usize) -> Result<(), RuntimeError> {
        self.reserved = self
            .reserved
            .checked_add(bytes)
            .filter(|sum| *sum <= self.maximum)
            .ok_or_else(|| RuntimeError::limit(LimitKind::FormatBytes, self.maximum))?;
        Ok(())
    }

    fn copy(&mut self, value: &Value) -> Result<Value, RuntimeError> {
        if let Value::String(text) = value {
            // Reject before cloning a host-provided oversized String.
            if text.len() > self.maximum.saturating_sub(self.reserved) {
                return Err(RuntimeError::limit(LimitKind::FormatBytes, self.maximum));
            }
            (self.charge)(text.len())?;
        }
        Ok(value.clone())
    }

    fn append_reserved(&mut self, text: &str) -> Result<(), RuntimeError> {
        (self.charge)(text.len())?;
        self.output.push_str(text);
        Ok(())
    }

    fn append(&mut self, text: &str) -> Result<(), RuntimeError> {
        self.reserve(text.len())?;
        self.append_reserved(text)
    }

    fn scalar(&mut self, value: &Value) -> Result<(), RuntimeError> {
        match value {
            Value::String(text) => self.append(text),
            Value::Function(function) => {
                self.append("<function")?;
                if let Some(name) = &function.name {
                    self.append(" ")?;
                    self.append(name)?;
                }
                self.append(">")
            }
            Value::NativeFunction(function) => {
                self.append("<function ")?;
                self.append(function.name())?;
                self.append(">")
            }
            Value::Module(module) => {
                self.append("<module ")?;
                self.append(&module.name)?;
                self.append(">")
            }
            _ => self.append(&value.scalar_string()),
        }
    }

    fn compare(
        &mut self,
        left: &Range<usize>,
        right: &Range<usize>,
    ) -> Result<Ordering, RuntimeError> {
        let left = self.output[left.clone()].as_bytes();
        let right = self.output[right.clone()].as_bytes();
        // Chunk comparisons allow cancellation during long shared prefixes.
        for (a, b) in left.chunks(256).zip(right.chunks(256)) {
            (self.charge)(a.len().min(b.len()).saturating_add(1))?;
            let order = a.cmp(b);
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(left.len().cmp(&right.len()))
    }

    fn order_entries(&mut self, entries: &mut Vec<Range<usize>>) -> Result<(), RuntimeError> {
        if entries.len() < 2 {
            return Ok(());
        }
        // A fallible bottom-up merge sort can stop immediately on budget failure.
        // A standard sort comparator cannot report a structured interruption.
        let mut scratch = Vec::with_capacity(entries.len());
        let mut width = 1usize;
        while width < entries.len() {
            (self.charge)(entries.len())?;
            scratch.clear();
            for chunk in entries.chunks(width.saturating_mul(2)) {
                let middle = width.min(chunk.len());
                let (mut left, mut right) = (0, middle);
                while left < middle && right < chunk.len() {
                    if self.compare(&chunk[left], &chunk[right])? != Ordering::Greater {
                        scratch.push(chunk[left].clone());
                        left += 1;
                    } else {
                        scratch.push(chunk[right].clone());
                        right += 1;
                    }
                }
                scratch.extend_from_slice(&chunk[left..middle]);
                scratch.extend_from_slice(&chunk[right..]);
            }
            std::mem::swap(entries, &mut scratch);
            width = width.saturating_mul(2);
        }
        // Already ordered entries need no descendant copying.
        if entries.windows(2).all(|pair| pair[0].start < pair[1].start) {
            return Ok(());
        }
        let start = entries.iter().map(|range| range.start).min().unwrap();
        let end = self.output.len();
        (self.charge)((end - start).saturating_mul(2))?;
        let mut ordered = String::with_capacity(end - start);
        for (index, range) in entries.iter().enumerate() {
            if index != 0 {
                ordered.push_str(", ");
            }
            ordered.push_str(&self.output[range.clone()]);
        }
        debug_assert_eq!(ordered.len(), end - start);
        self.output.truncate(start);
        self.output.push_str(&ordered);
        Ok(())
    }
}

pub(crate) fn render_values(
    values: &[Value],
    separator: &str,
    suffix: &str,
    maximum: usize,
    charge: impl FnMut(usize) -> Result<(), RuntimeError>,
) -> Result<String, RuntimeError> {
    let mut budget = Budget {
        maximum,
        reserved: 0,
        output: String::new(),
        charge,
    };
    (budget.charge)(0)?;
    budget.reserve(
        values
            .len()
            .saturating_sub(1)
            .saturating_mul(separator.len()),
    )?;
    budget.reserve(suffix.len())?;
    let mut active = HashSet::new();
    let mut tasks = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            budget.append_reserved(separator)?;
        }
        tasks.push(Task::Visit(budget.copy(value)?));
        while let Some(task) = tasks.pop() {
            (budget.charge)(1)?;
            match task {
                Task::Visit(value) => match &value {
                    Value::List(items) => {
                        let id = Id::List(Rc::as_ptr(items) as usize);
                        if !active.insert(id) {
                            budget.append("<cycle>")?;
                            continue;
                        }
                        let len = read_collection(items, "formatting")?.len();
                        budget.reserve(2)?;
                        budget.reserve(len.saturating_sub(1).saturating_mul(2))?;
                        budget.append_reserved("[")?;
                        tasks.push(Task::List(items.clone(), 0, len));
                    }
                    Value::Object(properties) => {
                        let id = Id::Object(Rc::as_ptr(properties) as usize);
                        if !active.insert(id) {
                            budget.append("<cycle>")?;
                            continue;
                        }
                        let properties_ref = read_collection(properties, "formatting")?;
                        budget.reserve(2)?;
                        budget.reserve(properties_ref.len().saturating_sub(1).saturating_mul(2))?;
                        let mut keys = Vec::new();
                        for key in properties_ref.keys() {
                            budget.reserve(key.len().saturating_add(3))?;
                            (budget.charge)(key.len().saturating_add(1))?;
                            keys.push(key.clone());
                        }
                        budget.append_reserved("{")?;
                        tasks.push(Task::Object {
                            properties: properties.clone(),
                            keys,
                            next: 0,
                            start: budget.output.len(),
                            entries: Vec::new(),
                        });
                    }
                    _ => budget.scalar(&value)?,
                },
                Task::List(items, next, len) => {
                    if next < len {
                        if next != 0 {
                            budget.append_reserved(", ")?;
                        }
                        let value = budget.copy(&read_collection(&items, "formatting")?[next])?;
                        tasks.push(Task::List(items, next + 1, len));
                        tasks.push(Task::Visit(value));
                    } else {
                        budget.append_reserved("]")?;
                        active.remove(&Id::List(Rc::as_ptr(&items) as usize));
                    }
                }
                Task::Object {
                    properties,
                    keys,
                    next,
                    start,
                    mut entries,
                } => {
                    if next != 0 {
                        entries.push(start..budget.output.len());
                    }
                    if next < keys.len() {
                        if next != 0 {
                            budget.append_reserved(", ")?;
                        }
                        let start = budget.output.len();
                        budget.append_reserved(&keys[next])?;
                        budget.append_reserved(" = ")?;
                        let value = budget
                            .copy(&read_collection(&properties, "formatting")?[&keys[next]])?;
                        tasks.push(Task::Object {
                            properties,
                            keys,
                            next: next + 1,
                            start,
                            entries,
                        });
                        tasks.push(Task::Visit(value));
                    } else {
                        budget.order_entries(&mut entries)?;
                        budget.append_reserved("}")?;
                        active.remove(&Id::Object(Rc::as_ptr(&properties) as usize));
                    }
                }
            }
        }
    }
    budget.append_reserved(suffix)?;
    debug_assert_eq!(budget.output.len(), budget.reserved);
    Ok(budget.output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::error::RuntimeErrorKind;

    fn rendered(value: &Value) -> String {
        render_values(std::slice::from_ref(value), "", "", 32_768, |_| Ok(())).unwrap()
    }

    #[test]
    fn complete_entry_order_matches_host_formatting_even_for_prefix_keys() {
        let keys = ["a", "a ", "a = ", "a = z", "", "\0", "é", "😀", "a = []"];
        let values = [
            Value::String("z".into()),
            Value::list(vec![Value::Nil, Value::String("tail".into())]),
            Value::String("".into()),
            Value::Integer(9223372036854775807),
        ];
        for offset in 0..keys.len() {
            let object = Value::object(
                keys.iter()
                    .enumerate()
                    .map(|(index, key)| {
                        (
                            (*key).to_owned(),
                            values[(index + offset) % values.len()].clone(),
                        )
                    })
                    .collect(),
            );
            let nested = Value::list(vec![
                object.clone(),
                Value::object(HashMap::from([
                    ("before".into(), object),
                    ("after".into(), Value::String("value".into())),
                ])),
            ]);
            assert_eq!(rendered(&nested), nested.to_solx_string());
        }
    }

    #[test]
    fn sorting_long_common_prefixes_is_fallible_and_preserves_byte_order() {
        let prefix = "é".repeat(300);
        let object = Value::object(
            (0..12)
                .map(|index| (format!("{prefix}{index:02}"), Value::Integer(index)))
                .collect(),
        );
        assert_eq!(rendered(&object), object.to_solx_string());
        let mut units = 0usize;
        let error = render_values(&[object], "", "", 32_768, |charge| {
            units += charge;
            if units > 15_000 {
                Err(RuntimeError::cancelled())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    }

    #[test]
    fn oversized_host_strings_are_rejected_before_copy_charges() {
        let value = Value::list(vec![Value::String("large".repeat(20))]);
        let mut work = 0;
        let error = render_values(&[value], "", "", 8, |units| {
            work += units;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(
            error.kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::FormatBytes)
        );
        assert!(work < 10);
    }
}
