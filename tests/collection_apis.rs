use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind};
use solx::runtime::{Value, error::RuntimeErrorKind};

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(&format!("import 'list' import 'core' {source}"))
        .unwrap();
    interpreter
}

fn numbers(values: &[i64]) -> Value {
    Value::list(values.iter().copied().map(Value::Integer).collect())
}

#[test]
fn map_uses_closures_native_callbacks_and_preserves_nil_results() {
    let interpreter = run(
        "fn make(offset) return fn(value) return value + offset end end \
         global mapped = list.map([1, 2, 3], make(5)) \
         global native = list.map([1, 2], str) \
         global nils = list.map([1, nil], fn(value) return nil end) \
         global closures = list.map([10, 20], fn(value) return fn() return value end end) \
         global captured = [closures[1](), closures[2]()]",
    );
    assert_eq!(interpreter.get_global("mapped"), Some(numbers(&[6, 7, 8])));
    assert_eq!(
        interpreter.get_global("native"),
        Some(Value::list(vec![
            Value::String("1".into()),
            Value::String("2".into())
        ]))
    );
    assert_eq!(
        interpreter.get_global("nils"),
        Some(Value::list(vec![Value::Nil, Value::Nil]))
    );
    assert_eq!(interpreter.get_global("captured"), Some(numbers(&[10, 20])));
}

#[test]
fn filter_uses_solx_truthiness_and_preserves_order() {
    let interpreter = run(
        "global result = list.filter([nil, false, 0, '', [], {}, true], fn(value) return value end)",
    );
    assert_eq!(
        interpreter.get_global("result"),
        Some(Value::list(vec![
            Value::Integer(0),
            Value::String(String::new()),
            Value::list(vec![]),
            Value::object(Default::default()),
            Value::Boolean(true),
        ]))
    );
}

#[test]
fn fold_is_left_to_right_can_store_nil_and_returns_the_empty_initial_value() {
    let interpreter = run(
        "global left = list.fold([1, 2, 3], 100, fn(acc, value) return acc - value end) \
         global text = list.fold([1, 2, 3], '', fn(acc, value) return acc .. value end) \
         global optional = list.fold([1, 2], 99, fn(acc, value) \
             if value == 2 and acc == nil then return 77 end return nil end) \
         global initial = {} global empty = list.fold([], initial, fn() end)",
    );
    assert_eq!(interpreter.get_global("left"), Some(Value::Integer(94)));
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("123".into()))
    );
    assert_eq!(interpreter.get_global("optional"), Some(Value::Integer(77)));
    let (Value::Object(initial), Value::Object(empty)) = (
        interpreter.get_global("initial").unwrap(),
        interpreter.get_global("empty").unwrap(),
    ) else {
        unreachable!()
    };
    assert!(Rc::ptr_eq(&initial, &empty));
}

#[test]
fn map_filter_and_sorted_always_return_new_lists_including_empty_inputs() {
    for (operation, callback) in [
        ("map", "str"),
        ("filter", "str"),
        ("sorted", "fn(a, b) return a - b end"),
    ] {
        for source in ["[]", "[1, 2]"] {
            let interpreter = run(&format!(
                "global input = {source} global output = list.{operation}(input, {callback})"
            ));
            let (Value::List(input), Value::List(output)) = (
                interpreter.get_global("input").unwrap(),
                interpreter.get_global("output").unwrap(),
            ) else {
                unreachable!()
            };
            assert!(!Rc::ptr_eq(&input, &output));
        }
    }
}

#[test]
fn map_filter_and_fold_visit_original_snapshot_membership_after_mutations() {
    for operation in ["map", "filter", "fold"] {
        let invocation = match operation {
            "map" => "list.map(input, fn(value) change(value) return value end)",
            "filter" => "list.filter(input, fn(value) change(value) return true end)",
            _ => {
                "list.fold(input, [], fn(acc, value) change(value) list.append(acc, value) return acc end)"
            }
        };
        let interpreter = run(&format!(
            "global input = [1, 2, 3] \
             fn change(value) if value == 1 then \
                 list.append(input, 99) input[2] = 20 list.remove(input, 3) end end \
             global output = {invocation}",
        ));
        assert_eq!(
            interpreter.get_global("output"),
            Some(numbers(&[1, 2, 3])),
            "{operation}"
        );
        assert_eq!(
            interpreter.get_global("input"),
            Some(numbers(&[1, 20, 99])),
            "{operation}"
        );
    }
}

#[test]
fn snapshot_is_shallow_and_callbacks_can_mutate_shared_elements() {
    let interpreter = run("global child = {value = 0} global input = [child, child] \
         global output = list.map(input, fn(value) value.value = value.value + 1 return value end) \
         global seen = [input[1].value, output[2].value] child.value = 9 \
         global after = output[1].value");
    assert_eq!(interpreter.get_global("seen"), Some(numbers(&[2, 2])));
    assert_eq!(interpreter.get_global("after"), Some(Value::Integer(9)));
}

#[test]
fn sorted_is_stable_and_does_not_reorder_the_input() {
    let interpreter = run("import 'string' \
         global input = [{key = 2, id = 'a'}, {key = 1, id = 'b'}, {key = 2, id = 'c'}, {key = 1, id = 'd'}] \
         global output = list.sorted(input, fn(a, b) \
             if a.key < b.key then return -0.25 elseif a.key > b.key then return 0.25 end return -0.0 end) \
         fn id(value) return value.id end \
         global ordered = string.join(list.map(output, id), '') \
         global original = string.join(list.map(input, id), '') \
         global extreme = list.sorted([3, 1, 2], fn(a, b) \
             if a < b then return -9223372036854775808 elseif a > b then return 9223372036854775807 end return 0 end)");
    assert_eq!(
        interpreter.get_global("ordered"),
        Some(Value::String("bdac".into()))
    );
    assert_eq!(
        interpreter.get_global("original"),
        Some(Value::String("abcd".into()))
    );
    assert_eq!(interpreter.get_global("extreme"), Some(numbers(&[1, 2, 3])));
}

#[test]
fn sorting_uses_snapshot_membership_and_retains_comparator_side_effects() {
    let interpreter = run("global input = [3, 1, 2] global calls = 0 \
         global output = list.sorted(input, fn(a, b) calls = calls + 1 \
             if calls == 1 then input[1] = 99 list.append(input, 88) list.remove(input, 2) end \
             return a - b end)");
    assert_eq!(interpreter.get_global("output"), Some(numbers(&[1, 2, 3])));
    assert_eq!(interpreter.get_global("input"), Some(numbers(&[99, 2, 88])));
}

#[test]
fn all_higher_order_functions_validate_arity_and_types_even_for_empty_inputs() {
    let mut interpreter = run("");
    for (operation, count) in [("map", 2), ("filter", 2), ("fold", 3), ("sorted", 2)] {
        assert_eq!(
            interpreter
                .run_source(&format!("list.{operation}()"))
                .unwrap_err()
                .message,
            format!("list.{operation}() expects {count} argument(s), got 0")
        );
        assert_eq!(
            interpreter
                .run_source(&format!("list.{operation}(nil, nil, nil, nil)"))
                .unwrap_err()
                .message,
            format!("list.{operation}() expects {count} argument(s), got 4")
        );
        let middle = if operation == "fold" { "nil, " } else { "" };
        assert_eq!(
            interpreter
                .run_source(&format!("list.{operation}(1, {middle}str)"))
                .unwrap_err()
                .message,
            format!("list.{operation}() argument 1 requires List, got Number")
        );
        assert_eq!(
            interpreter
                .run_source(&format!("list.{operation}([], {middle}nil)"))
                .unwrap_err()
                .message,
            format!("list.{operation}() argument {count} requires Function, got Nil")
        );
    }
}

#[test]
fn callbacks_reuse_user_and_native_arity_rules_without_calling_empty_inputs() {
    let mut interpreter = run("import 'string' fn pair(a, b) return a end");
    for (source, message) in [
        ("list.map([1], pair)", "pair() expects 2 argument(s), got 1"),
        (
            "list.filter([1], string.find)",
            "string.find() expects 2 argument(s), got 1",
        ),
        (
            "list.fold([1], 0, num)",
            "num() expects 1 argument(s), got 2",
        ),
        (
            "list.sorted([1, 2], core.type)",
            "core.type() expects 1 argument(s), got 2",
        ),
    ] {
        assert_eq!(interpreter.run_source(source).unwrap_err().message, message);
    }
    interpreter.run_source(
        "list.map([], pair) list.filter([], pair) list.fold([], nil, num) list.sorted([], core.type) list.sorted([1], core.type)",
    ).unwrap();
}

#[test]
fn callback_errors_retain_source_traces_effects_and_restore_caller_scopes() {
    let mut interpreter = run(
        "global visible = 'outer' global calls = 0 global input = [1, 2, 3]\n\
         fn fail(value)\n\
           calls = calls + 1\n\
           if value == 2 then core.error('callback failed') end\n\
           return value\n\
         end\n\
         fn outer() var visible = 'inner' return list.map(input, fail) end",
    );
    let error = interpreter.run_source("outer()").unwrap_err();
    assert_eq!(error.message, "callback failed");
    assert_eq!(error.location.unwrap().span.unwrap().line, 4);
    assert_eq!(
        error
            .contexts
            .iter()
            .map(|context| context.description.as_str())
            .collect::<Vec<_>>(),
        ["in fail()", "in outer()"]
    );
    assert_eq!(interpreter.get_global("calls"), Some(Value::Integer(2)));
    assert_eq!(interpreter.get_global("input"), Some(numbers(&[1, 2, 3])));
    interpreter
        .run_source("global restored = visible global recovered = list.map([4], fail)")
        .unwrap();
    assert_eq!(
        interpreter.get_global("restored"),
        Some(Value::String("outer".into()))
    );
    assert_eq!(interpreter.get_global("recovered"), Some(numbers(&[4])));
}

#[test]
fn sorting_rejects_bad_comparator_results_without_helper_mutation() {
    let mut interpreter = run("global input = [4, 3, 2, 1] global calls = 0");
    for (value, kind) in [
        ("nil", "Nil"),
        ("true", "Boolean"),
        ("'bad'", "String"),
        ("[]", "List"),
        ("{}", "Object"),
        ("str", "Function"),
        ("list", "Module"),
    ] {
        interpreter.run_source("calls = 0").unwrap();
        let source = format!(
            "list.sorted(input, fn(a, b) calls = calls + 1 if calls == 2 then return {value} end return a - b end)"
        );
        assert_eq!(
            interpreter.run_source(&source).unwrap_err().message,
            format!("list.sorted() comparator must return a finite Number, got {kind}")
        );
        assert_eq!(
            interpreter.get_global("input"),
            Some(numbers(&[4, 3, 2, 1]))
        );
        assert_eq!(interpreter.get_global("calls"), Some(Value::Integer(2)));
    }
    let error = interpreter
        .run_source(
            "calls = 0 list.sorted(input, fn(a, b) calls = calls + 1 input[1] = 99 \
         if calls == 2 then core.error('comparison failed') end return a - b end)",
        )
        .unwrap_err();
    assert_eq!(error.message, "comparison failed");
    assert_eq!(
        interpreter.get_global("input"),
        Some(numbers(&[99, 3, 2, 1]))
    );
}

#[test]
fn sorting_rejects_nonfinite_host_results_without_numeric_conversion() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut interpreter = run("global host = [nil] global input = [2, 1]");
        let Value::List(host) = interpreter.get_global("host").unwrap() else {
            unreachable!()
        };
        host.borrow_mut()[0] = Value::Number(number);
        assert_eq!(
            interpreter
                .run_source("list.sorted(input, fn(a, b) return host[1] end)")
                .unwrap_err()
                .message,
            "list.sorted() comparator must return a finite Number"
        );
        assert_eq!(interpreter.get_global("input"), Some(numbers(&[2, 1])));
    }
}

#[test]
fn inconsistent_comparators_terminate_with_a_permutation_and_bounded_calls() {
    let interpreter = run(
        "global input = [] for i = 1, 128 do list.append(input, i) end global calls = 0 \
         global output = list.sorted(input, fn(a, b) calls = calls + 1 \
             if calls % 3 == 0 then return 0 elseif calls % 2 == 0 then return -1 end return 1 end)",
    );
    let Value::List(output) = interpreter.get_global("output").unwrap() else {
        unreachable!()
    };
    let mut numbers: Vec<_> = output
        .borrow()
        .iter()
        .map(|value| {
            let Value::Integer(value) = value else {
                unreachable!()
            };
            *value
        })
        .collect();
    numbers.sort_unstable();
    assert_eq!(numbers, (1..=128).collect::<Vec<_>>());
    let Value::Integer(calls) = interpreter.get_global("calls").unwrap() else {
        unreachable!()
    };
    assert!(calls <= 128 * 7);
}

#[test]
fn snapshots_report_host_borrow_conflicts_and_release_borrows_before_callbacks() {
    let mut interpreter = run(
        "global input = [1, 2] global calls = 0 fn callback(value) calls = calls + 1 return value end",
    );
    let Value::List(items) = interpreter.get_global("input").unwrap() else {
        unreachable!()
    };
    for (operation, tail) in [
        ("map", "callback"),
        ("filter", "callback"),
        ("fold", "0, fn(a, b) return a + b end"),
        ("sorted", "fn(a, b) return a - b end"),
    ] {
        let guard = items.borrow_mut();
        assert_eq!(
            interpreter
                .run_source(&format!("list.{operation}(input, {tail})"))
                .unwrap_err()
                .message,
            format!("list.{operation}(): collection is mutably borrowed")
        );
        drop(guard);
    }
    assert_eq!(interpreter.get_global("calls"), Some(Value::Integer(0)));
    {
        let _guard = items.borrow();
        interpreter.run_source("list.map(input, callback)").unwrap();
    }
    interpreter
        .run_source("list.map(input, fn(value) list.append(input, value) return value end)")
        .unwrap();
    assert_eq!(items.borrow().len(), 4);
}

#[test]
fn snapshot_copying_and_native_callback_visits_consume_the_shared_step_budget() {
    for budget in [17, 18] {
        let mut interpreter = Interpreter::with_limits(ExecutionLimits {
            max_steps: Some(budget),
            ..ExecutionLimits::default()
        })
        .unwrap();
        interpreter
            .run_source("import 'list' global input = [1, 2, 3, 4]")
            .unwrap();
        let result = interpreter.run_source("list.map(input, str)");
        if budget == 17 {
            assert_eq!(
                result.unwrap_err().kind,
                RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
            );
        } else {
            result.unwrap();
        }
        assert_eq!(
            interpreter.get_global("input"),
            Some(numbers(&[1, 2, 3, 4]))
        );
    }
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: Some(30),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.run_source("import 'list' global input = [] global calls = 0 fn visit(value) calls = calls + 1 return value end").unwrap();
    let Value::List(input) = interpreter.get_global("input").unwrap() else {
        unreachable!()
    };
    input.borrow_mut().extend((0..1000).map(Value::Integer));
    assert_eq!(
        interpreter
            .run_source("list.map(input, visit)")
            .unwrap_err()
            .kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
    );
    assert_eq!(interpreter.get_global("calls"), Some(Value::Integer(0)));
    interpreter.run_source("global recovered = 1").unwrap();
}

fn interrupted_source(operation: &str) -> String {
    let parameters = if operation == "map" || operation == "filter" {
        "value"
    } else {
        "a, b"
    };
    let middle = if operation == "fold" { "0, " } else { "" };
    format!(
        "fn spin({parameters}) global touched = true while true do end end \
        try list.{operation}([2, 1], {middle}spin) catch err global caught = true end"
    )
}

#[test]
fn callback_deadlines_are_uncatchable_and_restore_the_interpreter() {
    for operation in ["map", "filter", "fold", "sorted"] {
        let mut interpreter = Interpreter::with_limits(ExecutionLimits {
            max_steps: None,
            max_duration: Some(Duration::from_millis(20)),
            ..ExecutionLimits::default()
        })
        .unwrap();
        interpreter.run_source("import 'list'").unwrap();
        let error = interpreter
            .run_source(&interrupted_source(operation))
            .unwrap_err();
        assert_eq!(
            error.kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)
        );
        assert!(!error.is_catchable());
        assert!(interpreter.get_global("caught").is_none());
        interpreter.run_source("global recovered = 1").unwrap();
    }
}

#[test]
fn callback_cancellation_is_sticky_and_can_be_reset_after_unwinding() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: None,
        max_duration: Some(Duration::from_secs(5)),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.run_source("import 'list'").unwrap();
    let token = interpreter.cancellation_token();
    let controller = token.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        controller.cancel();
    });
    let error = interpreter
        .run_source(&interrupted_source("fold"))
        .unwrap_err();
    thread.join().unwrap();
    assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    assert!(interpreter.get_global("caught").is_none());
    assert_eq!(
        interpreter.run_source("1").unwrap_err().kind,
        RuntimeErrorKind::Cancelled
    );
    token.reset();
    interpreter
        .run_source("global recovered = list.map([1], str)")
        .unwrap();
}

#[test]
fn recursively_invoked_callbacks_share_user_call_depth_limits() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_call_depth: 3,
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter
        .run_source("import 'list' fn recurse(value) return list.map([value], recurse) end")
        .unwrap();
    let error = interpreter
        .run_source("list.map([1], recurse)")
        .unwrap_err();
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::CallDepth)
    );
    assert_eq!(error.contexts.len(), 4);
    interpreter.run_source("global recovered = 1").unwrap();
}

#[test]
fn recursive_native_comparators_hit_depth_limits_instead_of_aborting() {
    let source = "import 'list' var items = [] list.append(items, items) list.append(items, list.sorted) list.sorted(items, list.sorted)";
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["-e", source])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("recursive native callback exceeded ten seconds");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("evaluation depth limit exceeded"),
        "{stderr}"
    );
    assert!(!stderr.contains("stack overflow"));
    assert!(output.stdout.is_empty());
}
