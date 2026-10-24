//! Every stress case runs in a watched subprocess, so a stack abort cannot kill
//! the parent harness. Values are deliberately much deeper than the AST limits.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{BufRead, Read, Write},
    rc::{Rc, Weak},
    sync::mpsc,
};

use solx::{
    interpreter::Interpreter,
    limits::{ExecutionLimits, LimitKind},
    runtime::{Environment, Value, collect_garbage, error::RuntimeErrorKind, track_value},
};

const DEPTH: usize = 100_000;
const CANCELLATION_READY: &str = "SOLX_DEEP_CANCELLATION_READY";

enum Probe {
    List(Weak<RefCell<Vec<Value>>>),
    Object(Weak<RefCell<HashMap<String, Value>>>),
}

impl Probe {
    fn new(value: &Value) -> Self {
        match value {
            Value::List(value) => Self::List(Rc::downgrade(value)),
            Value::Object(value) => Self::Object(Rc::downgrade(value)),
            _ => unreachable!(),
        }
    }

    fn alive(&self) -> bool {
        match self {
            Self::List(value) => value.strong_count() != 0,
            Self::Object(value) => value.strong_count() != 0,
        }
    }
}

fn chain(depth: usize, objects: bool) -> (Value, Vec<Probe>) {
    let mut value = Value::Integer(42);
    let mut probes = Vec::with_capacity(depth);
    for index in 0..depth {
        // Raw, unregistered host payloads also need stack-safe destruction.
        value = if objects && index % 2 == 0 {
            Value::Object(Rc::new(RefCell::new(HashMap::from([(
                "child".into(),
                value,
            )]))))
        } else {
            Value::List(Rc::new(RefCell::new(vec![value])))
        };
        probes.push(Probe::new(&value));
    }
    (value, probes)
}

fn in_child(name: &str, case: impl FnOnce() + Send + 'static) {
    in_child_with_watcher(name, case, watched);
}

fn in_child_with_watcher(name: &str, case: impl FnOnce() + Send + 'static, watch: fn(Command)) {
    if std::env::var("SOLX_DESTRUCTION_CHILD").as_deref() == Ok(name) {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(case)
            .unwrap()
            .join()
            .unwrap();
    } else {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", name, "--nocapture"]);
        command.env("SOLX_DESTRUCTION_CHILD", name);
        watch(command);
    }
}

// Cancel only after interpreted code has transferred both values into its local
// scope. The stdin request crosses the process boundary without sharing Rc data
// between threads or depending on how quickly the interpreter gets scheduled.
fn watched_cancellation(mut command: Command) {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take();
    let output = child.stdout.take().unwrap();
    let mut errors = child.stderr.take().unwrap();
    let (ready, readiness) = mpsc::channel();
    let stdout = std::thread::spawn(move || {
        let mut transcript = Vec::new();
        let mut output = std::io::BufReader::new(output);
        let mut line = Vec::new();
        loop {
            line.clear();
            if output.read_until(b'\n', &mut line).unwrap() == 0 {
                break;
            }
            if String::from_utf8_lossy(&line).trim() == CANCELLATION_READY {
                let _ = ready.send(());
            }
            transcript.extend_from_slice(&line);
        }
        transcript
    });
    let stderr = std::thread::spawn(move || {
        let mut transcript = Vec::new();
        errors.read_to_end(&mut transcript).unwrap();
        transcript
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut cancellation_sent = false;
    let mut timed_out = false;
    let mut request_error = None;
    let status = loop {
        if !cancellation_sent && readiness.try_recv().is_ok() {
            let mut input = input.take().unwrap();
            if let Err(error) = input.write_all(b"cancel\n").and_then(|()| input.flush()) {
                request_error = Some(error);
                let _ = child.kill();
                break child.wait().unwrap();
            }
            cancellation_sent = true;
        }
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(input);
    let stdout = stdout.join().unwrap();
    let stderr = stderr.join().unwrap();
    assert!(
        !timed_out && request_error.is_none() && cancellation_sent && status.success(),
        "cancellation child failed: {status:?}, timed_out={timed_out}, request_error={request_error:?}, cancellation_sent={cancellation_sent}\n{}\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
}

fn watched(mut command: Command) {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "child failed: {:?}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "child exceeded 30s: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn language_assignment_releases_deep_lists() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_solx"));
    command.args(["-e", "var x=nil for i=1,20000 do x=[x] end x=nil"]);
    watched(command);
}

#[test]
fn raw_host_lists_release_every_node_on_a_small_stack() {
    in_child("raw_host_lists_release_every_node_on_a_small_stack", || {
        let (root, probes) = chain(DEPTH, false);
        drop(root);
        assert!(probes.iter().all(|probe| !probe.alive()));
    });
}

#[test]
fn mixed_objects_and_lists_release_through_a_raw_rc_owner() {
    in_child(
        "mixed_objects_and_lists_release_through_a_raw_rc_owner",
        || {
            let (root, probes) = chain(DEPTH, true);
            let Value::List(items) = &root else {
                unreachable!()
            };
            let owner = items.clone();
            drop(root);
            assert!(probes.iter().all(Probe::alive));
            drop(owner);
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn shared_descendants_and_borrows_keep_identity_and_contents() {
    in_child(
        "shared_descendants_and_borrows_keep_identity_and_contents",
        || {
            let (root, probes) = chain(DEPTH, true);
            let alias = root.clone();
            let Value::List(items) = &alias else {
                unreachable!()
            };
            let guard = items.borrow_mut();
            let parents = Value::list(vec![root, alias.clone()]);
            drop(parents);
            assert!(probes.iter().all(Probe::alive));
            assert_eq!(guard.len(), 1);
            drop(guard);
            drop(alias);
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn replacement_and_removal_release_children_under_parent_borrows() {
    in_child(
        "replacement_and_removal_release_children_under_parent_borrows",
        || {
            let (first, probes) = chain(DEPTH, true);
            let parent = Value::list(vec![first]);
            let Value::List(items) = &parent else {
                unreachable!()
            };
            let mut guard = items.borrow_mut();
            guard[0] = Value::Nil;
            assert!(probes.iter().all(|probe| !probe.alive()));
            let (second, probes) = chain(DEPTH, false);
            guard.push(second);
            drop(guard.remove(1));
            assert!(probes.iter().all(|probe| !probe.alive()));

            let (third, probes) = chain(DEPTH, true);
            let object = Value::object(HashMap::from([("child".into(), third)]));
            let Value::Object(properties) = &object else {
                unreachable!()
            };
            drop(properties.borrow_mut().remove("child"));
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn collection_reclaims_cycles_but_preserves_shared_deep_roots() {
    in_child(
        "collection_reclaims_cycles_but_preserves_shared_deep_roots",
        || {
            let (root, probes) = chain(DEPTH, true);
            let cycle = Value::list(vec![root.clone()]);
            let Value::List(items) = &cycle else {
                unreachable!()
            };
            items.borrow_mut().push(cycle.clone());
            let cycle_probe = Probe::new(&cycle);
            track_value(&cycle);
            drop(cycle);
            collect_garbage();
            assert!(!cycle_probe.alive());
            assert!(probes.iter().all(Probe::alive));
            drop(root);
            assert!(probes.iter().all(|probe| !probe.alive()));
            assert_eq!(collect_garbage().remaining, 0);

            let (cycle, probes) = chain(20_000, true);
            let mut leaf = cycle.clone();
            for _ in 1..20_000 {
                leaf = match &leaf {
                    Value::List(items) => items.borrow()[0].clone(),
                    Value::Object(properties) => properties.borrow()["child"].clone(),
                    _ => unreachable!(),
                };
            }
            let Value::Object(properties) = &leaf else {
                unreachable!()
            };
            properties.borrow_mut().insert("back".into(), cycle.clone());
            track_value(&cycle);
            drop(leaf);
            drop(cycle);
            assert!(probes.iter().all(Probe::alive));
            collect_garbage();
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn environment_release_and_failed_binding_reclaim_deep_values() {
    in_child(
        "environment_release_and_failed_binding_reclaim_deep_values",
        || {
            let (root, probes) = chain(DEPTH, true);
            let mut environment = Environment::new();
            environment
                .define("deep".into(), root, false, None, false)
                .unwrap();
            let clone = environment.clone();
            drop(environment);
            assert!(probes.iter().all(Probe::alive));
            drop(clone);
            assert!(probes.iter().all(|probe| !probe.alive()));
            let (root, probes) = chain(DEPTH, true);
            let mut environment = Environment::new();
            assert!(
                environment
                    .define("wrong".into(), root, false, Some("Number".into()), false)
                    .is_err()
            );
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn language_error_and_return_cleanup_release_deep_locals() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_solx"));
    command.args(["-e", "import 'core' import 'list' import 'object' fn build() var x=nil for i=1,20000 do x={child=x} end return x end var x=build() var saved=x var box=[x] x=nil list.remove(box,1) core.assert(saved.child~=nil) saved=nil try var doomed=build() core.error('expected') catch err core.assert(err.message=='expected') end var record={child=build()} object.remove(record, 'child')"]);
    watched(command);
}

#[test]
fn execution_budget_unwinds_deep_values_and_recovers() {
    in_child("execution_budget_unwinds_deep_values_and_recovers", || {
        let mut interpreter = Interpreter::with_limits(ExecutionLimits {
            max_steps: Some(400_000),
            ..ExecutionLimits::default()
        })
        .unwrap();
        let error = interpreter
            .run_source(
                "fn build() var x=nil for i=1,20000 do x=[x] end while true do end end build()",
            )
            .unwrap_err();
        assert_eq!(
            error.kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
        );
        interpreter.run_source("global recovered=42").unwrap();
        assert_eq!(
            interpreter.get_global("recovered"),
            Some(Value::Integer(42))
        );
    });
}

#[test]
fn leaked_refcell_borrow_flags_do_not_prevent_owned_destruction() {
    in_child(
        "leaked_refcell_borrow_flags_do_not_prevent_owned_destruction",
        || {
            let (root, probes) = chain(DEPTH, false);
            let Value::List(items) = &root else {
                unreachable!()
            };
            // Forget only the guard; unlike forgetting the Value, this owns no Rc.
            std::mem::forget(items.borrow_mut());
            drop(root);
            assert!(probes.iter().all(|probe| !probe.alive()));
        },
    );
}

#[test]
fn cancellation_unwinds_deep_locals_and_preserves_host_roots() {
    in_child_with_watcher(
        "cancellation_unwinds_deep_locals_and_preserves_host_roots",
        || {
            let mut interpreter = Interpreter::with_limits(ExecutionLimits {
                max_steps: None,
                max_duration: Some(Duration::from_secs(10)),
                ..ExecutionLimits::default()
            })
            .unwrap();
            interpreter
                .run_source(&format!(
                    "global slot=[nil,nil] fn consume() var local=slot[1] var shared=slot[2] slot[1]=nil slot[2]=nil print('\\n{CANCELLATION_READY}') while true do end end"
                ))
                .unwrap();
            let (root, probes) = chain(DEPTH, true);
            track_value(&root);
            // One deep graph must be reclaimed during cancellation. A separate,
            // smaller shared graph must survive while the host owns its alias.
            let (retained, retained_probes) = chain(32, true);
            track_value(&retained);
            let slot = interpreter.get_global("slot").unwrap();
            let Value::List(items) = &slot else {
                unreachable!()
            };
            items.borrow_mut()[0] = root;
            items.borrow_mut()[1] = retained.clone();
            // Keep registration-pressure collection outside the execution deadline.
            collect_garbage();
            let token = interpreter.cancellation_token();
            let controller = token.clone();
            let worker = std::thread::spawn(move || {
                let mut request = String::new();
                std::io::stdin().read_line(&mut request).unwrap();
                assert_eq!(request, "cancel\n");
                controller.cancel();
            });
            let error = interpreter.run_source("consume()").unwrap_err();
            assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
            worker.join().unwrap();
            assert!(
                items
                    .borrow()
                    .iter()
                    .all(|value| matches!(value, Value::Nil)),
                "execution transferred both values into locals before cancellation"
            );
            assert!(probes.iter().all(|probe| !probe.alive()));
            assert!(retained_probes.iter().all(Probe::alive));
            token.reset();
            interpreter.run_source("global recovered=42").unwrap();
            assert_eq!(
                interpreter.get_global("recovered"),
                Some(Value::Integer(42))
            );
            drop(retained);
            assert!(retained_probes.iter().all(|probe| !probe.alive()));
        },
        watched_cancellation,
    );
}
