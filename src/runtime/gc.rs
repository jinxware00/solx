//! Cycle collection for the existing single-threaded Rc value graph.
//!
//! Each snapshot owns exactly one Rc per node. Internal edges are counted with
//! multiplicity; any remaining strong references are roots (including Rust
//! callers, evaluator temporaries, environments, and module caches). We trace
//! from those roots and sever only unreachable mutable edges. No user code or
//! destructors outside Solx's closed Value types run during this pass.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use super::environment::{Binding, Scope};
use super::value::{ModuleValue, UserFunction, Value};

type List = Rc<RefCell<Vec<Value>>>;
type Object = Rc<RefCell<HashMap<String, Value>>>;

/// Node counts for one collection, not bytes or process memory measurements.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CollectionStats {
    pub examined: usize,
    pub reclaimed: usize,
    pub remaining: usize,
    /// Borrowed containers retained conservatively until a later pass.
    pub borrowed: usize,
    /// Cumulative collection passes on this thread, including this pass.
    pub collections: usize,
    /// Cumulative nodes reclaimed by collection on this thread.
    pub total_reclaimed: usize,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Id {
    List(usize),
    Object(usize),
    Scope(usize),
    Function(usize),
    Module(usize),
}

pub(super) enum Node {
    List(List),
    Object(Object),
    Scope(Scope),
    Function(Rc<UserFunction>),
    Module(Rc<ModuleValue>),
}

enum WeakNode {
    List(Weak<RefCell<Vec<Value>>>),
    Object(Weak<RefCell<HashMap<String, Value>>>),
    Scope(Weak<RefCell<HashMap<String, Binding>>>),
    Function(Weak<UserFunction>),
    Module(Weak<ModuleValue>),
}

impl Node {
    pub(super) fn from_value(value: &Value) -> Option<Self> {
        Some(match value {
            Value::List(value) => Self::List(value.clone()),
            Value::Object(value) => Self::Object(value.clone()),
            Value::Function(value) => Self::Function(value.clone()),
            Value::Module(value) => Self::Module(value.clone()),
            _ => return None,
        })
    }

    fn id(&self) -> Id {
        match self {
            Self::List(v) => Id::List(Rc::as_ptr(v) as usize),
            Self::Object(v) => Id::Object(Rc::as_ptr(v) as usize),
            Self::Scope(v) => Id::Scope(Rc::as_ptr(v) as usize),
            Self::Function(v) => Id::Function(Rc::as_ptr(v) as usize),
            Self::Module(v) => Id::Module(Rc::as_ptr(v) as usize),
        }
    }

    fn downgrade(&self) -> WeakNode {
        match self {
            Self::List(v) => WeakNode::List(Rc::downgrade(v)),
            Self::Object(v) => WeakNode::Object(Rc::downgrade(v)),
            Self::Scope(v) => WeakNode::Scope(Rc::downgrade(v)),
            Self::Function(v) => WeakNode::Function(Rc::downgrade(v)),
            Self::Module(v) => WeakNode::Module(Rc::downgrade(v)),
        }
    }

    fn strong_count(&self) -> usize {
        match self {
            Self::List(v) => Rc::strong_count(v),
            Self::Object(v) => Rc::strong_count(v),
            Self::Scope(v) => Rc::strong_count(v),
            Self::Function(v) => Rc::strong_count(v),
            Self::Module(v) => Rc::strong_count(v),
        }
    }

    // Probe exclusive access even though this only reads. A borrowed container
    // is a conservative root, and its uncounted outgoing edges root its children.
    // Thus both shared and mutable host guards are safe without panics or writes.
    fn children(&self) -> Option<Vec<Node>> {
        Some(match self {
            Self::List(v) => v
                .try_borrow_mut()
                .ok()?
                .iter()
                .filter_map(Self::from_value)
                .collect(),
            Self::Object(v) => v
                .try_borrow_mut()
                .ok()?
                .values()
                .filter_map(Self::from_value)
                .collect(),
            Self::Scope(v) => v
                .try_borrow_mut()
                .ok()?
                .values()
                .filter_map(|b| Self::from_value(&b.value))
                .collect(),
            Self::Function(v) => v.closure.collection_scopes(),
            // A module and an exposed exports Object must refer to ONE node.
            Self::Module(v) => vec![Self::Object(v.exports.clone())],
        })
    }

    fn clear(&self) {
        // The same containers were exclusively borrowable during tracing; no
        // callbacks, allocation hooks, or concurrent mutations run between phases.
        match self {
            Self::List(v) => v.borrow_mut().clear(),
            Self::Object(v) => v.borrow_mut().clear(),
            Self::Scope(v) => v.borrow_mut().clear(),
            Self::Function(_) | Self::Module(_) => {}
        }
    }
}

impl WeakNode {
    fn upgrade(&self) -> Option<Node> {
        match self {
            Self::List(v) => v.upgrade().map(Node::List),
            Self::Object(v) => v.upgrade().map(Node::Object),
            Self::Scope(v) => v.upgrade().map(Node::Scope),
            Self::Function(v) => v.upgrade().map(Node::Function),
            Self::Module(v) => v.upgrade().map(Node::Module),
        }
    }

    fn alive(&self) -> bool {
        match self {
            Self::List(v) => v.strong_count() != 0,
            Self::Object(v) => v.strong_count() != 0,
            Self::Scope(v) => v.strong_count() != 0,
            Self::Function(v) => v.strong_count() != 0,
            Self::Module(v) => v.strong_count() != 0,
        }
    }
}

struct Registry {
    nodes: HashMap<Id, WeakNode>,
    allocations: usize,
    threshold: usize,
    collections: usize,
    total_reclaimed: usize,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            allocations: 0,
            threshold: 512,
            collections: 0,
            total_reclaimed: 0,
        }
    }
}

impl Registry {
    fn register(&mut self, first: Node, revisit: bool) {
        let mut pending = vec![first];
        let mut visited = HashSet::new();
        while let Some(node) = pending.pop() {
            let id = node.id();
            if !visited.insert(id) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(entry) = self.nodes.entry(id) {
                entry.insert(node.downgrade());
                self.allocations = self.allocations.saturating_add(1);
            } else if !revisit {
                continue;
            }
            if let Some(children) = node.children() {
                pending.extend(children);
            }
        }
    }

    fn collect(&mut self) -> CollectionStats {
        self.collections = self.collections.saturating_add(1);
        self.nodes.retain(|_, node| node.alive());
        let mut nodes = Vec::new();
        let mut ids = HashMap::new();
        for node in self.nodes.values().filter_map(WeakNode::upgrade) {
            ids.insert(node.id(), nodes.len());
            nodes.push(node);
        }
        let mut edges = Vec::new();
        let mut borrowed = Vec::new();
        let mut cursor = 0;
        while cursor < nodes.len() {
            let children = nodes[cursor].children();
            borrowed.push(children.is_none());
            let mut outgoing = Vec::new();
            for child in children.into_iter().flatten() {
                let next = nodes.len();
                let index = *ids.entry(child.id()).or_insert(next);
                if index == next {
                    self.nodes.insert(child.id(), child.downgrade());
                    nodes.push(child);
                }
                // Do not deduplicate: two fields own two strong references.
                outgoing.push(index);
            }
            edges.push(outgoing);
            cursor += 1;
        }

        let mut incoming = vec![0usize; nodes.len()];
        for &child in edges.iter().flatten() {
            incoming[child] += 1;
        }
        let mut live = vec![false; nodes.len()];
        let mut pending = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            let strong = node.strong_count();
            // A violated edge-count invariant must retain memory, never erase it.
            if strong < incoming[index] + 1 {
                return CollectionStats {
                    examined: nodes.len(),
                    remaining: nodes.len(),
                    borrowed: borrowed.iter().filter(|&&b| b).count(),
                    reclaimed: 0,
                    collections: self.collections,
                    total_reclaimed: self.total_reclaimed,
                };
            }
            if borrowed[index] || strong > incoming[index] + 1 {
                pending.push(index);
            }
        }
        while let Some(index) = pending.pop() {
            if live[index] {
                continue;
            }
            live[index] = true;
            pending.extend(edges[index].iter().copied());
        }

        for (index, node) in nodes.iter().enumerate() {
            if !live[index] {
                node.clear();
            }
        }
        let examined = nodes.len();
        // Snapshot roots prevent recursive graph destruction while edges clear.
        drop(nodes);
        self.nodes.retain(|_, node| node.alive());
        let reclaimed = examined - self.nodes.len();
        self.total_reclaimed = self.total_reclaimed.saturating_add(reclaimed);
        if self.nodes.capacity() > self.nodes.len().saturating_mul(4).max(1024) {
            self.nodes.shrink_to(self.nodes.len().max(512));
        }
        self.allocations = 0;
        // Avoid full-heap scans every 512 allocations when the live heap is large.
        self.threshold = self.nodes.len().max(512);
        CollectionStats {
            examined,
            reclaimed,
            remaining: self.nodes.len(),
            borrowed: borrowed.iter().filter(|&&b| b).count(),
            collections: self.collections,
            total_reclaimed: self.total_reclaimed,
        }
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(super) fn register(node: Node) {
    let _ = REGISTRY.try_with(|registry| registry.borrow_mut().register(node, false));
}

/// Adopt raw host-created Rc graphs, including edges added through public payloads.
/// Release host RefCell guards before calling this to visit all descendants.
pub fn track_value(value: &Value) {
    if let Some(node) = Node::from_value(value) {
        let _ = REGISTRY.try_with(|registry| registry.borrow_mut().register(node, true));
    }
}

/// Collect unreachable tracked cycles on this thread. All external strong Rc
/// references are roots, so host-owned Values and other interpreters remain live.
/// Weak references do not root values. Borrowed containers are retained safely.
pub fn collect_garbage() -> CollectionStats {
    REGISTRY
        .try_with(|registry| registry.borrow_mut().collect())
        .unwrap_or_default()
}

pub(crate) fn maybe_collect() {
    let _ = REGISTRY.try_with(|registry| {
        let mut registry = registry.borrow_mut();
        if registry.allocations >= registry.threshold {
            registry.collect();
        }
    });
}
