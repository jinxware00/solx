use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use solx::interpreter::Interpreter;
use solx::runtime::{Environment, Value, collect_garbage, track_value};

enum Probe {
    List(Weak<RefCell<Vec<Value>>>),
    Object(Weak<RefCell<HashMap<String, Value>>>),
}

impl Probe {
    fn of(value: &Value) -> Self {
        match value {
            Value::List(v) => Self::List(Rc::downgrade(v)),
            Value::Object(v) => Self::Object(Rc::downgrade(v)),
            _ => panic!("expected collection"),
        }
    }

    fn alive(&self) -> bool {
        match self {
            Self::List(v) => v.strong_count() != 0,
            Self::Object(v) => v.strong_count() != 0,
        }
    }
}

fn connect(parent: &Value, index: usize, child: Value) {
    match parent {
        Value::List(v) => v.borrow_mut().push(child),
        Value::Object(v) => {
            v.borrow_mut().insert(index.to_string(), child);
        }
        _ => panic!("expected collection"),
    }
}

fn cycle() -> Value {
    let value = Value::list(vec![]);
    connect(&value, 0, value.clone());
    value
}

#[test]
fn reclaims_self_cycles_and_mixed_graphs_with_duplicate_edges() {
    collect_garbage();
    let list = cycle();
    let object = Value::object(HashMap::new());
    connect(&object, 0, list.clone());
    connect(&object, 1, list.clone());
    connect(&object, 2, object.clone());
    connect(&list, 1, object.clone());
    let probes = [Probe::of(&list), Probe::of(&object)];
    drop(list);
    drop(object);
    let stats = collect_garbage();
    assert_eq!(stats.reclaimed, 2);
    assert!(probes.iter().all(|probe| !probe.alive()));
    assert_eq!(collect_garbage().reclaimed, 0);
}

#[test]
fn host_roots_keep_their_whole_graph_live_until_the_last_clone_is_dropped() {
    let root = cycle();
    let child = Value::object(HashMap::new());
    connect(&child, 0, root.clone());
    connect(&root, 1, child.clone());
    let probe = Probe::of(&child);
    let alias = root.clone();
    drop(root);
    drop(child);
    collect_garbage();
    assert!(probe.alive());
    assert_eq!(alias.to_string(), "[<cycle>, {0 = <cycle>}]");
    drop(alias);
    collect_garbage();
    assert!(!probe.alive());
}

#[test]
fn weak_handles_do_not_keep_values_alive() {
    let value = cycle();
    let probe = Probe::of(&value);
    drop(value);
    collect_garbage();
    assert!(!probe.alive());
}

#[test]
fn acyclic_values_still_release_without_waiting_for_collection() {
    let value = Value::list(vec![Value::object(HashMap::new())]);
    let probe = Probe::of(&value);
    drop(value);
    assert!(!probe.alive());
}

#[test]
fn shared_and_mutable_host_borrows_are_conservative_without_blocking_other_garbage() {
    for mutable in [false, true] {
        let value = cycle();
        let child = cycle();
        let child_probe = Probe::of(&child);
        connect(&value, 1, child);
        let Value::List(items) = &value else {
            unreachable!()
        };
        let garbage = cycle();
        let garbage_probe = Probe::of(&garbage);
        drop(garbage);
        if mutable {
            let mut guard = items.borrow_mut();
            let stats = collect_garbage();
            assert!(stats.borrowed >= 1);
            assert_eq!(guard.len(), 2);
            guard.push(Value::Number(42.0));
        } else {
            let guard = items.borrow();
            let stats = collect_garbage();
            assert!(stats.borrowed >= 1);
            assert_eq!(guard.len(), 2);
        }
        assert!(!garbage_probe.alive());
        assert!(child_probe.alive());
        drop(value);
        collect_garbage();
        assert!(!child_probe.alive());
    }
}

#[test]
fn raw_host_graphs_can_be_adopted_including_after_direct_mutation() {
    let root = Value::list(vec![]);
    let raw = Value::Object(Rc::new(RefCell::new(HashMap::new())));
    connect(&raw, 0, raw.clone());
    let probe = Probe::of(&raw);
    connect(&root, 0, raw);
    track_value(&root);
    // Disconnect before the next collection: registration must already exist.
    let Value::List(items) = &root else {
        unreachable!()
    };
    items.borrow_mut().clear();
    collect_garbage();
    assert!(!probe.alive());
    drop(root);
}

#[test]
fn constructor_adopts_new_raw_descendants_and_collection_discovers_later_edges() {
    let raw = Value::List(Rc::new(RefCell::new(vec![])));
    connect(&raw, 0, raw.clone());
    let raw_probe = Probe::of(&raw);
    let parent = Value::list(vec![raw]);
    drop(parent);
    collect_garbage();
    assert!(!raw_probe.alive());

    let parent = cycle();
    let raw = Value::Object(Rc::new(RefCell::new(HashMap::new())));
    connect(&raw, 0, raw.clone());
    let raw_probe = Probe::of(&raw);
    connect(&parent, 1, raw);
    drop(parent);
    collect_garbage();
    assert!(!raw_probe.alive());
}

#[test]
fn public_environment_clones_are_roots_and_track_host_values() {
    let mut environment = Environment::new();
    let raw = Value::List(Rc::new(RefCell::new(vec![])));
    connect(&raw, 0, raw.clone());
    let probe = Probe::of(&raw);
    environment
        .define("value".into(), raw, false, None, false)
        .unwrap();
    let mut clone = environment.clone();
    drop(environment);
    collect_garbage();
    assert_eq!(clone.get("value").unwrap().to_string(), "[<cycle>]");
    clone.assign("value", Value::Nil).unwrap();
    collect_garbage();
    assert!(!probe.alive());
}

#[test]
fn reference_graph_oracle_checks_mixed_cycles_aliases_and_root_removal() {
    // Deterministic graph generation; ten independent components guarantee both
    // reachable and unreachable cycles, with repeated edges and self references.
    for seed in 1..=20u64 {
        let nodes: Vec<_> = (0..60)
            .map(|i| {
                if i % 2 == 0 {
                    Value::list(vec![])
                } else {
                    Value::object(HashMap::new())
                }
            })
            .collect();
        let probes: Vec<_> = nodes.iter().map(Probe::of).collect();
        let mut edges = vec![Vec::new(); nodes.len()];
        let mut random = seed;
        for i in 0..nodes.len() {
            for edge in 0..4 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let target = (i / 6) * 6 + (random as usize % 6);
                edges[i].push(target);
                connect(&nodes[i], edge, nodes[target].clone());
            }
        }
        let mut live = vec![false; nodes.len()];
        let mut pending = vec![0, 17, 51];
        while let Some(index) = pending.pop() {
            if live[index] {
                continue;
            }
            live[index] = true;
            pending.extend(edges[index].iter().copied());
        }
        let roots = [nodes[0].clone(), nodes[17].clone(), nodes[51].clone()];
        drop(nodes);
        collect_garbage();
        for (index, probe) in probes.iter().enumerate() {
            assert_eq!(probe.alive(), live[index], "seed {seed}, node {index}");
        }
        drop(roots);
        collect_garbage();
        assert!(probes.iter().all(|probe| !probe.alive()), "seed {seed}");
    }
}

#[test]
fn large_unreachable_cycle_is_cleared_without_recursive_graph_destruction() {
    let nodes: Vec<_> = (0..10_000).map(|_| Value::list(vec![])).collect();
    for i in 0..nodes.len() {
        connect(&nodes[i], 0, nodes[(i + 1) % nodes.len()].clone());
    }
    let probes: Vec<_> = nodes.iter().map(Probe::of).collect();
    drop(nodes);
    let stats = collect_garbage();
    assert!(stats.reclaimed >= 10_000);
    assert!(probes.iter().all(|probe| !probe.alive()));
}

#[test]
fn automatic_collection_reclaims_detached_cycles_before_a_program_finishes() {
    let before = collect_garbage();
    let detached = cycle();
    let probe = Probe::of(&detached);
    drop(detached);
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'core' import 'list'
        global live = []
        list.append(live, live)
        for i = 1, 1600 do
            var temporary = []
            list.append(temporary, temporary)
        end
        core.assert(#live == 1)",
        )
        .unwrap();
    assert!(!probe.alive());
    let after = collect_garbage();
    // A final run_program pass and one after each top-level statement cannot
    // explain these passes: collection must happen repeatedly inside the loop.
    assert!(after.collections - before.collections >= 5);
    assert!(after.total_reclaimed - before.total_reclaimed >= 1600);
    let live = interpreter.get_global("live").unwrap();
    assert_eq!(live.to_string(), "[<cycle>]");
    let live_probe = Probe::of(&live);
    drop(interpreter);
    collect_garbage();
    assert!(live_probe.alive());
    drop(live);
    collect_garbage();
    assert!(!live_probe.alive());
}

#[test]
fn closures_and_mutual_recursion_survive_collection_and_are_reclaimed_after_drop() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'core'
        fn make()
            var count = 0
            fn even(n) if n == 0 then return true end return odd(n - 1) end
            fn odd(n) if n == 0 then return false end return even(n - 1) end
            return { step = fn() count = count + 1 return count end, even = even }
        end
        global saved = make()",
        )
        .unwrap();
    let saved = interpreter.get_global("saved").unwrap();
    let probe = Probe::of(&saved);
    drop(saved);
    for _ in 0..3 {
        collect_garbage();
    }
    interpreter.run_source("core.assert(saved.step() == 1) core.assert(saved.step() == 2) core.assert(saved.even(12)) saved = nil").unwrap();
    assert!(!probe.alive());
}

#[test]
fn host_function_reference_keeps_captured_values_after_interpreter_drop() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("global items = [42] global keep = fn() return items end")
        .unwrap();
    let probe = Probe::of(&interpreter.get_global("items").unwrap());
    let function = interpreter.get_global("keep").unwrap();
    let Value::Function(rc) = &function else {
        unreachable!()
    };
    let function_probe = Rc::downgrade(rc);
    drop(interpreter);
    collect_garbage();
    assert!(probe.alive());
    assert!(function_probe.upgrade().is_some());
    drop(function);
    collect_garbage();
    assert!(!probe.alive());
    assert!(function_probe.upgrade().is_none());
}

#[test]
fn collecting_one_interpreter_does_not_clear_another_interpreters_state() {
    let mut first = Interpreter::new();
    let mut second = Interpreter::new();
    first
        .run_source("global value = [1] fn f() return value end")
        .unwrap();
    second
        .run_source("import 'core' global value = [2] fn f() return value end")
        .unwrap();
    let first_probe = Probe::of(&first.get_global("value").unwrap());
    drop(first);
    assert!(!first_probe.alive());
    second.run_source("core.assert(f()[1] == 2)").unwrap();
}

#[test]
fn evaluator_temporaries_and_returned_closures_are_roots_during_nested_collection() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'core' import 'list'
        fn churn()
            for i = 1, 700 do var c = [] list.append(c, c) end
            return 7
        end
        fn make()
            var items = [42]
            return fn() return items[1] end
        end
        fn consume(first, second)
            core.assert(first[1]() == 42)
            core.assert(second == 7)
            return first[1]
        end
        var returned = consume([make()], churn())
        core.assert(returned() == 42)
        core.assert(([make(), churn()])[1]() == 42)
        var record = { keep = make(), other = churn() }
        core.assert(record.keep() == 42)",
        )
        .unwrap();
}

#[test]
fn error_unwinding_and_catch_closures_remain_valid_under_collection_pressure() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'core' import 'list'
        fn recover()
            try
                for i = 1, 700 do var c = [] list.append(c, c) end
                core.error('captured')
            catch err
                return fn() return err.message end
            end
        end
        global saved = recover()",
        )
        .unwrap();
    collect_garbage();
    interpreter
        .run_source("core.assert(saved() == 'captured')")
        .unwrap();
    let error = interpreter
        .run_source("fn failing() var c = [] list.append(c, c) num('bad') end failing()")
        .unwrap_err();
    assert_eq!(error.message, "cannot convert 'bad' to Number");
    collect_garbage();
    interpreter
        .run_source("core.assert(saved() == 'captured')")
        .unwrap();
}

#[test]
fn registration_metadata_does_not_accumulate_after_repeated_collection() {
    collect_garbage();
    for _ in 0..10 {
        for _ in 0..1000 {
            drop(cycle());
        }
        let stats = collect_garbage();
        assert_eq!(stats.reclaimed, 1000);
        assert_eq!(stats.remaining, 0);
    }
}
