//! Callback-based list operations. Snapshot membership before calling user code;
//! all copying, visiting and merge movement uses the shared execution control.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use super::Interpreter;
use crate::runtime::{RuntimeError, Value, maybe_collect, read_collection};

type List = Rc<RefCell<Vec<Value>>>;
type ResultValue = Result<Value, RuntimeError>;

impl Interpreter {
    fn snapshot_list(&mut self, items: &List, operation: &str) -> Result<Vec<Value>, RuntimeError> {
        let items = read_collection(items, operation)?;
        let mut snapshot = Vec::new();
        for value in items.iter() {
            self.control.step()?;
            snapshot.push(value.clone());
        }
        Ok(snapshot)
    }

    pub(crate) fn map_list(&mut self, items: &List, callback: &Value) -> ResultValue {
        let snapshot = self.snapshot_list(items, "list.map()")?;
        let mut output = Vec::new();
        for value in snapshot {
            self.control.step()?;
            output.push(self.invoke_callback(callback, vec![value])?);
            maybe_collect();
        }
        Ok(Value::list(output))
    }

    pub(crate) fn filter_list(&mut self, items: &List, callback: &Value) -> ResultValue {
        let snapshot = self.snapshot_list(items, "list.filter()")?;
        let mut output = Vec::new();
        for value in snapshot {
            self.control.step()?;
            if self
                .invoke_callback(callback, vec![value.clone()])?
                .is_truthy()
            {
                output.push(value);
            }
            maybe_collect();
        }
        Ok(Value::list(output))
    }

    pub(crate) fn fold_list(
        &mut self,
        items: &List,
        initial: &Value,
        callback: &Value,
    ) -> ResultValue {
        let snapshot = self.snapshot_list(items, "list.fold()")?;
        let mut accumulated = initial.clone();
        for value in snapshot {
            self.control.step()?;
            accumulated = self.invoke_callback(callback, vec![accumulated, value])?;
            maybe_collect();
        }
        Ok(accumulated)
    }

    pub(crate) fn sorted_list(&mut self, items: &List, comparator: &Value) -> ResultValue {
        let mut values = self.snapshot_list(items, "list.sorted()")?;
        let length = values.len();
        let mut width = 1usize;
        // Bottom-up mergesort has no recursive sort frames and tolerates an
        // inconsistent comparator without the panics possible in standard sort.
        while width < length {
            let mut merged = Vec::new();
            let mut start = 0;
            while start < length {
                let middle = start.saturating_add(width).min(length);
                let end = middle.saturating_add(width).min(length);
                let (mut left, mut right) = (start, middle);
                while left < middle && right < end {
                    self.control.step()?;
                    let compared = self.invoke_callback(
                        comparator,
                        vec![values[left].clone(), values[right].clone()],
                    )?;
                    let order = comparator_order(&compared)?;
                    let index = if order != Ordering::Greater {
                        let index = left;
                        left += 1;
                        index
                    } else {
                        let index = right;
                        right += 1;
                        index
                    };
                    merged.push(std::mem::replace(&mut values[index], Value::Nil));
                    maybe_collect();
                }
                for index in (left..middle).chain(right..end) {
                    self.control.step()?;
                    merged.push(std::mem::replace(&mut values[index], Value::Nil));
                }
                start = end;
            }
            values = merged;
            width = width.saturating_mul(2);
        }
        Ok(Value::list(values))
    }
}

fn comparator_order(value: &Value) -> Result<Ordering, RuntimeError> {
    match value {
        Value::Integer(number) => Ok(number.cmp(&0)),
        Value::Number(number) if number.is_finite() => {
            Ok(number.partial_cmp(&0.0).expect("finite comparator result"))
        }
        Value::Number(_) => Err(RuntimeError::new(
            "list.sorted() comparator must return a finite Number",
        )),
        other => Err(RuntimeError::new(format!(
            "list.sorted() comparator must return a finite Number, got {}",
            other.type_name()
        ))),
    }
}
