use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind};
use solx::parser::ast::{Expr, Program, Stmt};
use solx::runtime::{Value, error::RuntimeErrorKind};
use std::time::Duration;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(&format!(
            "import 'core' import 'list' import 'object'\n{source}"
        ))
        .unwrap();
    interpreter
}

#[test]
fn lists_yield_values_or_one_based_index_value_pairs_including_nil() {
    let interpreter = run("global values = []
         global indexed = []
         for value in [10, nil, false, 'last'] do list.append(values, value) end
         for index, value in values do list.append(indexed, [index, value]) end
         core.assert(values == [10, nil, false, 'last'])
         core.assert(indexed == [[1, 10], [2, nil], [3, false], [4, 'last']])");
    let Value::List(indexed) = interpreter.get_global("indexed").unwrap() else {
        panic!("expected list");
    };
    let Value::List(pair) = &indexed.borrow()[0] else {
        panic!("expected pair");
    };
    assert!(matches!(pair.borrow()[0], Value::Integer(1)));
}

#[test]
fn objects_yield_sorted_keys_or_key_value_pairs() {
    run("var data = { z = 3, a = nil, 'é' = 4, A = 1, '' = false }
         var keys = []
         var pairs = []
         for key in data do list.append(keys, key) end
         for key, value in data do list.append(pairs, [key, value]) end
         core.assert(keys == ['', 'A', 'a', 'z', 'é'])
         core.assert(pairs == [['', false], ['A', 1], ['a', nil], ['z', 3], ['é', 4]])");
}

#[test]
fn collection_expression_runs_once_before_binding_and_empty_collections_skip_body() {
    run("var value = [1, 2]
         global calls = 0
         fn source() calls = calls + 1 return value end
         var seen = []
         for value in source() do list.append(seen, value) end
         core.assert(calls == 1)
         core.assert(seen == [1, 2])
         core.assert(value == [1, 2])
         for missing in [] do core.error('empty list') end
         for missing, other in {} do core.error('empty object') end");
}

#[test]
fn list_mutations_and_rebinding_do_not_change_the_original_snapshot() {
    run("var data = [10, nil, 30]
         var visited = []
         for index, value in data do
             list.append(visited, [index, value])
             if index == 1 then
                 list.remove(data, 1)
                 data[1] = 99
                 data[2] = 300
                 list.append(data, 40)
                 data = [1000]
             end
             index = nil
             value = 'changed local'
         end
         core.assert(visited == [[1, 10], [2, nil], [3, 30]])
         core.assert(data == [1000])");
}

#[test]
fn object_mutations_do_not_change_snapshot_keys_or_values() {
    run("var data = { c = 30, b = nil, a = 10 }
         var visited = []
         for key, value in data do
             list.append(visited, [key, value])
             if key == 'a' then
                 object.remove(data, 'b')
                 data.c = 300
                 data.d = 40
                 data = nil
             end
         end
         core.assert(visited == [['a', 10], ['b', nil], ['c', 30]])
         core.assert(data == nil)");
}

#[test]
fn snapshots_share_nested_values_but_do_not_re_read_replaced_entries() {
    run("var data = [{ amount = 1 }, { amount = 2 }]
         var seen = []
         for index, value in data do
             if index == 1 then
                 data[2].amount = 20
                 data[2] = { amount = 200 }
             end
             list.append(seen, value.amount)
             value.amount = value.amount + 1
         end
         core.assert(seen == [1, 20])
         core.assert(data[1].amount == 2)
         core.assert(data[2].amount == 200)
         var inner = []
         for key, value in { nested = inner } do list.append(value, key) end
         core.assert(inner == ['nested'])");
}

#[test]
fn closures_keep_independent_iteration_bindings_and_shared_outer_state() {
    run("const index = 'outer index'
         const value = 'outer value'
         var saved = []
         var calls = 0
         for index, value in [10, 20, 30] do
             list.append(saved, fn()
                 calls = calls + 1
                 value = value + 1
                 return [index, value, calls]
             end)
         end
         core.assert(saved[1]() == [1, 11, 1])
         core.assert(saved[2]() == [2, 21, 2])
         core.assert(saved[1]() == [1, 12, 3])
         core.assert(saved[3]() == [3, 31, 4])
         core.assert(index == 'outer index')
         core.assert(value == 'outer value')
         var readers = []
         for key, entry in { b = 2, a = 1 } do
             list.append(readers, fn() return [key, entry] end)
         end
         core.assert(readers[1]() == ['a', 1])
         core.assert(readers[2]() == ['b', 2])");
}

#[test]
fn nested_loop_jumps_and_returns_restore_scopes() {
    run("var value = 'outer'
         var seen = []
         for value in [1, 2, 3, 4] do
             try
                 if value == 2 then continue end
                 if value == 4 then break end
                 for key, nested in { b = 20, a = 10 } do
                     if key == 'b' then break end
                     list.append(seen, [value, nested])
                 end
             catch err core.error('unexpected catch') end
         end
         core.assert(seen == [[1, 10], [3, 10]])
         core.assert(value == 'outer')
         fn early()
             for value in [7, 8] do
                 for key in { present = true } do return fn() return value end end
             end
         end
         fn bare() for value in [1] do return end end
         core.assert(early()() == 7)
         core.assert(bare() == nil)
         core.assert(value == 'outer')");
}

#[test]
fn body_errors_preserve_effects_captured_bindings_and_caller_scope() {
    let mut interpreter = run("var value = 'outer'
         global changed = 0
         global saved = nil
         fn fail()
             for value in [1, 2, 3] do
                 changed = value
                 saved = fn() return value end
                 if value == 2 then core.error('iteration failed') end
             end
         end");
    let error = interpreter.run_source("fail()").unwrap_err();
    assert_eq!(error.message, "iteration failed");
    assert!(error.is_catchable());
    interpreter
        .run_source(
            "core.assert(changed == 2) core.assert(saved() == 2) core.assert(value == 'outer')",
        )
        .unwrap();
    interpreter
        .run_source("try fail() catch err core.assert(err.message == 'iteration failed') end core.assert(value == 'outer')")
        .unwrap();
}

#[test]
fn invalid_collection_types_and_expired_bindings_report_errors() {
    for (expression, type_name) in [
        ("nil", "Nil"),
        ("true", "Boolean"),
        ("1", "Number"),
        ("'text'", "String"),
        ("fn() end", "Function"),
        ("core", "Module"),
    ] {
        let mut interpreter = run("global visited = false");
        let error = interpreter
            .run_source(&format!("for value in {expression} do visited = true end"))
            .unwrap_err();
        assert_eq!(
            error.message,
            format!("collection for requires List or Object, got {type_name}")
        );
        assert!(error.is_catchable());
        assert_eq!(
            interpreter.get_global("visited"),
            Some(Value::Boolean(false))
        );
    }
    for source in [
        "for value in [nil] do end print(value)",
        "for key, value in {} do end print(key)",
        "for key, value in {a = 1} do break end print(value)",
    ] {
        let error = Interpreter::new().run_source(source).unwrap_err();
        assert!(error.message.contains("undeclared variable"));
    }
}

#[test]
fn host_borrow_conflicts_are_recoverable_and_iteration_releases_its_own_borrow() {
    let mut interpreter = run("global data = [1, 2]");
    let Value::List(data) = interpreter.get_global("data").unwrap() else {
        panic!("expected list");
    };
    {
        let _guard = data.borrow_mut();
        let error = interpreter
            .run_source("for value in data do end")
            .unwrap_err();
        assert_eq!(
            error.message,
            "collection iteration: collection is mutably borrowed"
        );
    }
    interpreter
        .run_source("for value in data do list.remove(data, 1) end core.assert(#data == 0)")
        .unwrap();
    interpreter.run_source("global record = { a = 1 }").unwrap();
    let Value::Object(record) = interpreter.get_global("record").unwrap() else {
        panic!("expected object");
    };
    {
        let _guard = record.borrow_mut();
        let error = interpreter
            .run_source("for key in record do end")
            .unwrap_err();
        assert_eq!(
            error.message,
            "collection iteration: collection is mutably borrowed"
        );
    }
    interpreter
        .run_source("for key in record do object.remove(record, key) end core.assert(#record == 0)")
        .unwrap();
}

#[test]
fn snapshot_values_remain_gc_roots_after_the_source_is_dropped() {
    run("var data = [{ amount = 10 }, { amount = 20 }]
         var total = 0
         for index, value in data do
             data = nil
             for count = 1, 600 do
                 var garbage = []
                 list.append(garbage, garbage)
             end
             total = total + value.amount
         end
         core.assert(total == 30)
         var record = { a = { amount = 10 }, b = { amount = 20 } }
         total = 0
         for key, value in record do
             record = nil
             for count = 1, 600 do
                 var garbage = []
                 list.append(garbage, garbage)
             end
             total = total + value.amount
         end
         core.assert(total == 30)");
}

#[test]
fn snapshot_copying_and_empty_body_iterations_consume_the_execution_budget() {
    for expression in ["[1, 2, 3]", "{ a = 1, b = 2, c = 3 }"] {
        // Statement + iterable literal + three elements + three snapshot copies
        // + three empty-body iterations require exactly eleven steps.
        for (steps, succeeds) in [(10, false), (11, true)] {
            let mut interpreter = Interpreter::with_limits(ExecutionLimits {
                max_steps: Some(steps),
                ..ExecutionLimits::default()
            })
            .unwrap();
            let result = interpreter.run_source(&format!("for value in {expression} do end"));
            if succeeds {
                result.unwrap();
            } else {
                assert_eq!(
                    result.unwrap_err().kind,
                    RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
                );
            }
        }
    }
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: Some(5),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter
        .run_source("global data = [] global visited = false")
        .unwrap();
    let Value::List(data) = interpreter.get_global("data").unwrap() else {
        panic!("expected list");
    };
    data.borrow_mut().extend((0..100).map(Value::Integer));
    let error = interpreter
        .run_source("for value in data do visited = true break end")
        .unwrap_err();
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
    );
    assert_eq!(
        interpreter.get_global("visited"),
        Some(Value::Boolean(false))
    );
}

#[test]
fn iteration_interruption_bypasses_catch_and_restores_outer_bindings() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: Some(100),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter
        .run_source("global value = 7 global saved = nil")
        .unwrap();
    let error = interpreter.run_source(
        "try for value in [10, 20] do saved = fn() return value end while true do end end catch err global caught = true end"
    ).unwrap_err();
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
    );
    assert_eq!(interpreter.get_global("value"), Some(Value::Integer(7)));
    assert_eq!(interpreter.get_global("caught"), None);
    interpreter.run_source("global retained = saved()").unwrap();
    assert_eq!(interpreter.get_global("retained"), Some(Value::Integer(10)));
}

#[test]
fn another_thread_can_cancel_collection_iteration_and_execution_recovers() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: None,
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.run_source("global value = 'outer'").unwrap();
    let token = interpreter.cancellation_token();
    let controller = token.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        controller.cancel();
    });
    let error = interpreter.run_source("try while true do for value in [1, 2, 3] do end end catch err global caught = true end").unwrap_err();
    thread.join().unwrap();
    assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    assert_eq!(interpreter.get_global("caught"), None);
    assert_eq!(
        interpreter.get_global("value"),
        Some(Value::String("outer".into()))
    );
    token.reset();
    interpreter.run_source("global after = 42").unwrap();
}

#[test]
fn host_ast_duplicate_iteration_names_are_rejected() {
    let error = Interpreter::new()
        .run_program(&Program {
            statements: vec![Stmt::CollectionFor {
                name: "value".into(),
                second_name: Some("value".into()),
                iterable: Expr::List(vec![]),
                body: vec![],
            }],
        })
        .unwrap_err();
    assert_eq!(error.message, "duplicate loop variable 'value'");
}
