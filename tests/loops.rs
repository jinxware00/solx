use solx::interpreter::Interpreter;
use solx::runtime::Value;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(source).unwrap();
    interpreter
}

fn assert_number(interpreter: &Interpreter, name: &str, expected: f64) {
    assert_eq!(
        interpreter.get_global(name),
        Some(Value::Number(expected)),
        "{name}"
    );
}

#[test]
fn while_reevaluates_condition_outside_each_fresh_body_scope() {
    let interpreter = run(r#"
        var count = 0
        global checks = 0
        global total = 0
        fn condition() checks = checks + 1 return count < 3 end
        while condition() do
            count = count + 1
            const local = count
            total = total + local
            var count = 100
        end
        global final = count
        "#);
    assert_number(&interpreter, "checks", 4.0);
    assert_number(&interpreter, "total", 6.0);
    assert_number(&interpreter, "final", 3.0);
}

#[test]
fn while_uses_existing_truthiness_and_can_skip_the_body() {
    for (condition, expected) in [
        ("false", 0.0),
        ("nil", 0.0),
        ("0", 2.0),
        ("''", 2.0),
        ("[]", 2.0),
        ("true", 2.0),
    ] {
        let interpreter = run(&format!(
            "global visits = 0 while visits < 2 and {condition} do visits = visits + 1 end"
        ));
        assert_number(&interpreter, "visits", expected);
    }
}

#[test]
fn numeric_for_handles_inclusive_bounds_steps_and_empty_ranges() {
    for (header, expected) in [
        ("1, 3", "1,2,3,"),
        ("3, 1, -1", "3,2,1,"),
        ("1, 6, 2", "1,3,5,"),
        ("0.5, 1.5, 0.5", "0.5,1,1.5,"),
        ("1.5, 0.5, -0.5", "1.5,1,0.5,"),
        ("3, 1", ""),
        ("1, 3, -1", ""),
        ("2, 2", "2,"),
        ("2, 2, -1", "2,"),
    ] {
        let interpreter = run(&format!(
            r#"
            global trace = ""
            var visits = 0
            for i = {header} do
                visits = visits + 1
                if visits > 10 then break end
                trace = trace .. str(i) .. ","
            end
            "#
        ));
        assert_eq!(
            interpreter.get_global("trace"),
            Some(Value::String(expected.into())),
            "{header}"
        );
    }
}

#[test]
fn for_header_is_evaluated_once_left_to_right_before_counter_binding() {
    let interpreter = run(r#"
        var i = 2
        var limit = 4
        var step = 1
        global order = 0
        global total = 0
        fn mark(value, digit) order = order * 10 + digit return value end
        for i = mark(i, 1), mark(limit, 2), mark(step, 3) do
            total = total + i
            limit = 0
            step = 100
        end
        global outer = i
        "#);
    assert_number(&interpreter, "order", 123.0);
    assert_number(&interpreter, "total", 9.0);
    assert_number(&interpreter, "outer", 2.0);
}

#[test]
fn changing_or_deleting_visible_counter_does_not_change_iteration() {
    let interpreter = run(r#"
        var i = 50
        global total = 0
        global unmasked = 0
        for i = 1, 3 do
            total = total + i
            i = nil
            unmasked = unmasked + i
            var i = "local"
        end
        global outer = i
        "#);
    assert_number(&interpreter, "total", 6.0);
    assert_number(&interpreter, "unmasked", 150.0);
    assert_number(&interpreter, "outer", 50.0);
}

#[test]
fn break_and_continue_restore_nested_scopes_and_target_the_nearest_loop() {
    let interpreter = run(r#"
        var visible = 9
        global total = 0
        global afterInner = 0
        for i = 1, 4 do
            if i == 2 then
                var visible = 100
                continue
            end
            var j = 0
            while j < 4 do
                j = j + 1
                if j == 1 then continue end
                if j == 3 then var visible = 200 break end
                total = total + i * 10 + j
            end
            afterInner = afterInner + 1
            if i == 3 then break end
        end
        global restored = visible
        "#);
    assert_number(&interpreter, "total", 44.0);
    assert_number(&interpreter, "afterInner", 2.0);
    assert_number(&interpreter, "restored", 9.0);
}

#[test]
fn for_closures_keep_independent_iteration_bindings_and_shared_outer_state() {
    let interpreter = run(r#"
        var saved = [nil, nil, nil]
        var total = 0
        for i = 1, 3 do
            const initial = i
            saved[i] = fn()
                i = i + 10
                total = total + initial
                return i
            end
        end
        global first = saved[1]()
        global second = saved[2]()
        global third = saved[3]()
        global repeated = saved[1]()
        global shared = total
        "#);
    for (name, expected) in [
        ("first", 11.0),
        ("second", 12.0),
        ("third", 13.0),
        ("repeated", 21.0),
        ("shared", 7.0),
    ] {
        assert_number(&interpreter, name, expected);
    }
}

#[test]
fn while_closures_retain_locals_after_continue_and_break() {
    let interpreter = run(r#"
        var saved = [nil, nil, nil]
        var count = 0
        while count < 3 do
            count = count + 1
            var local = count
            saved[count] = fn() local = local + 10 return local end
            if count < 3 then continue end
            break
        end
        global first = saved[1]()
        global second = saved[2]()
        global third = saved[3]()
        global repeated = saved[1]()
        "#);
    for (name, expected) in [
        ("first", 11.0),
        ("second", 12.0),
        ("third", 13.0),
        ("repeated", 21.0),
    ] {
        assert_number(&interpreter, name, expected);
    }
}

#[test]
fn returns_cross_both_loop_kinds_and_preserve_returned_closures() {
    let interpreter = run(r#"
        fn find()
            for i = 1, 3 do
                var j = 0
                while j < 3 do
                    j = j + 1
                    if i == 2 and j == 2 then
                        return fn() return i * 10 + j end
                    end
                end
            end
            return fn() return -1 end
        end
        fn bare()
            for i = 1, 2 do
                var j = 0
                while j < 2 do
                    j = j + 1
                    return
                end
            end
            return 99
        end
        fn explicit()
            var attempts = 0
            while attempts < 2 and 0 do
                attempts = attempts + 1
                for i = 1, 2 do return nil end
            end
            return 99
        end
        global result = find()()
        global bareResult = bare()
        global nilResult = explicit()
        "#);
    assert_number(&interpreter, "result", 22.0);
    assert_eq!(interpreter.get_global("bareResult"), Some(Value::Nil));
    assert_eq!(interpreter.get_global("nilResult"), Some(Value::Nil));
}

#[test]
fn function_returns_and_inner_loop_control_do_not_exit_the_callers_loop() {
    let interpreter = run(r#"
        fn work(n)
            for j = 1, 3 do
                if j == 1 then continue end
                break
            end
            return n
        end
        global total = 0
        for i = 1, 3 do total = total + work(i) end
        "#);
    assert_number(&interpreter, "total", 6.0);
}

#[test]
fn invalid_for_bounds_report_errors_before_executing_the_body() {
    for (header, message) in [
        ("'1', 3", "for loop start requires Number, got String"),
        ("1, false", "for loop limit requires Number, got Boolean"),
        ("1, 3, nil", "for loop step requires Number, got Nil"),
        ("1, 3, 0", "for loop step must not be zero"),
        ("1, 3, -0", "for loop step must not be zero"),
        ("1 / 0, 3", "for loop start must be finite"),
        ("1, 0 / 0", "for loop limit must be finite"),
        ("1, 3, -1 / 0", "for loop step must be finite"),
        ("3, 1, 0", "for loop step must not be zero"),
    ] {
        let mut interpreter = run("global entered = false var i = 9");
        let error = interpreter
            .run_source(&format!("for i = {header} do entered = true break end"))
            .unwrap_err();
        assert_eq!(error.message, message, "{header}");
        assert_eq!(
            interpreter.get_global("entered"),
            Some(Value::Boolean(false))
        );
        interpreter.run_source("global restored = i").unwrap();
        assert_number(&interpreter, "restored", 9.0);
    }
}

#[test]
fn for_detects_nonadvancing_steps_without_hanging_and_handles_overflow() {
    for header in ["1e20, 2e20, 1", "-1e20, -2e20, -1"] {
        let mut interpreter = run("var i = 7 global visits = 0");
        // If the numeric progress check regresses, the body still stops the test.
        let error = interpreter
            .run_source(&format!(
                r#"
            for i = {header} do
                visits = visits + 1
                if visits > 2 then break end
            end
            "#
            ))
            .unwrap_err();
        assert_eq!(error.message, "for loop step does not advance the counter");
        assert_number(&interpreter, "visits", 1.0);
        interpreter.run_source("global restored = i").unwrap();
        assert_number(&interpreter, "restored", 7.0);
    }
    for header in [
        "1e20, 1e20, 1",
        "1e308, 1.7e308, 1e308",
        "-1e308, -1.7e308, -1e308",
    ] {
        let interpreter = run(&format!(
            "global visits = 0 for i = {header} do visits = visits + 1 if visits > 2 then break end end"
        ));
        assert_number(&interpreter, "visits", 1.0);
    }
}

#[test]
fn nested_loop_call_errors_restore_scopes_and_retain_escaped_closures() {
    let mut interpreter = run(r#"
        var visible = "file"
        var saved
        fn fail() return num("bad") end
        fn outer()
            var visible = "caller"
            for i = 1, 3 do
                var j = 0
                while j < 2 do
                    j = j + 1
                    if j == 1 then
                        var visible = "branch"
                        saved = fn() return i * 10 + j end
                        fail()
                    end
                end
            end
        end
        "#);
    assert_eq!(
        interpreter.run_source("outer()").unwrap_err().message,
        "cannot convert 'bad' to Number"
    );
    interpreter
        .run_source("global restored = visible global retained = saved()")
        .unwrap();
    assert_eq!(
        interpreter.get_global("restored"),
        Some(Value::String("file".into()))
    );
    assert_number(&interpreter, "retained", 11.0);
    for local in ["i", "j"] {
        assert!(
            interpreter
                .run_source(local)
                .unwrap_err()
                .message
                .contains("undeclared variable")
        );
    }
}

#[test]
fn loop_header_errors_restore_callers_and_stop_later_header_expressions() {
    for statement in [
        "while fail() do break end",
        "for i = fail(), mark(), mark() do break end",
        "for i = 1, fail(), mark() do break end",
        "for i = 1, 2, fail() do break end",
    ] {
        let mut interpreter = run(&format!(
            r#"
            var visible = "file"
            global effects = 0
            fn fail() var visible = "inner" return num("bad") end
            fn mark() effects = effects + 1 return 1 end
            fn outer() var visible = "caller" {statement} end
            "#
        ));
        assert_eq!(
            interpreter.run_source("outer()").unwrap_err().message,
            "cannot convert 'bad' to Number"
        );
        interpreter.run_source("global restored = visible").unwrap();
        assert_eq!(
            interpreter.get_global("restored"),
            Some(Value::String("file".into()))
        );
        assert_number(&interpreter, "effects", 0.0);
    }
}

#[test]
fn loop_body_errors_restore_top_level_scopes_and_preserve_binding_checks() {
    for loop_head in [
        "var attempts = 0 while attempts < 1 do attempts = attempts + 1",
        "for i = 1, 2 do",
    ] {
        for (declaration, action, expected) in [
            (
                "const value = 1",
                "value = nil",
                "cannot reassign constant 'value'",
            ),
            (
                "var value: Number = 1",
                "value = nil",
                "type mismatch for 'value': expected Number, got Nil",
            ),
        ] {
            let mut interpreter = run(&format!("var visible = 'file' {declaration}"));
            let error = interpreter
                .run_source(&format!(
                    "{loop_head} var visible = 'body' {action} break end"
                ))
                .unwrap_err();
            assert_eq!(error.message, expected);
            interpreter
                .run_source("global restored = visible global retained = value")
                .unwrap();
            assert_eq!(
                interpreter.get_global("restored"),
                Some(Value::String("file".into()))
            );
            assert_number(&interpreter, "retained", 1.0);
        }
    }
}

#[test]
fn manually_constructed_loop_control_cannot_escape_a_function_call() {
    use solx::parser::{ast::Stmt, parse};

    for jump in [Stmt::Break, Stmt::Continue] {
        let expected = match jump {
            Stmt::Break => "break is only allowed inside a loop",
            _ => "continue is only allowed inside a loop",
        };
        let mut program = parse(
            r#"
            fn bad() var visible = "inner" end
            var count = 0
            while count < 1 do count = count + 1 bad() end
            "#,
        )
        .unwrap();
        let Stmt::Function { function, .. } = program.statements[0].unlocated_mut() else {
            panic!("expected function declaration");
        };
        function.body.push(jump);
        let mut interpreter = run("var visible = 'file'");
        assert_eq!(
            interpreter.run_program(&program).unwrap_err().message,
            expected
        );
        interpreter.run_source("global restored = visible").unwrap();
        assert_eq!(
            interpreter.get_global("restored"),
            Some(Value::String("file".into()))
        );
    }
}
