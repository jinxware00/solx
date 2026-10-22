use solx::interpreter::Interpreter;
use solx::runtime::Value;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(source).unwrap();
    interpreter
}

fn assert_global(interpreter: &Interpreter, name: &str, expected: Value) {
    assert_eq!(interpreter.get_global(name), Some(expected), "{name}");
}

#[test]
fn named_and_anonymous_functions_return_values() {
    let interpreter = run(r#"
        fn add(a, b)
            return a + b
        end
        var multiplier = fn(x)
            return x * 2
        end
        global sum = add(10, 20)
        global product = multiplier(5)
        "#);
    assert_global(&interpreter, "sum", Value::Number(30.0));
    assert_global(&interpreter, "product", Value::Number(10.0));
}

#[test]
fn zero_argument_functions_and_all_nil_returns() {
    let interpreter = run(r#"
        fn empty() end
        fn bare() return end
        fn explicit() return nil end
        fn expressionOnly() 42 end
        fn early()
            return
            missing()
        end
        fn semicolons();;; return; missing(); end;
        fn identity(x) return x end
        global a = empty()
        global b = bare()
        global c = explicit()
        global d = expressionOnly()
        global e = early()
        global f = semicolons()
        global g = identity(nil)
        global h = fn() return 7 end()
        "#);
    for name in ["a", "b", "c", "d", "e", "f", "g"] {
        assert_global(&interpreter, name, Value::Nil);
    }
    assert_global(&interpreter, "h", Value::Number(7.0));
}

#[test]
fn returns_exit_nested_if_elseif_and_else_bodies() {
    let interpreter = run(r#"
        fn choose(value)
            if value == 1 then
                if true then return "first" end
                missing()
            elseif value == 2 then
                return "second"
            else
                return "last"
            end
            missing()
        end
        global first = choose(1)
        global second = choose(2)
        global last = choose(3)
        fn inner() return 4 end
        fn outer()
            var result = inner()
            return result + 1
        end
        global nested = outer()
        "#);
    assert_global(&interpreter, "first", Value::String("first".into()));
    assert_global(&interpreter, "second", Value::String("second".into()));
    assert_global(&interpreter, "last", Value::String("last".into()));
    assert_global(&interpreter, "nested", Value::Number(5.0));
}

#[test]
fn named_and_anonymous_functions_can_recurse() {
    let interpreter = run(r#"
        fn factorial(n)
            if n <= 1 then return 1 end
            return n * factorial(n - 1)
        end
        var fibonacci = fn(n)
            if n <= 1 then return n end
            return fibonacci(n - 1) + fibonacci(n - 2)
        end
        global fact = factorial(6)
        global fib = fibonacci(10)
        "#);
    assert_global(&interpreter, "fact", Value::Number(720.0));
    assert_global(&interpreter, "fib", Value::Number(55.0));
}

#[test]
fn nested_recursive_function_survives_its_defining_call() {
    let interpreter = run(r#"
        fn makeFactorial()
            fn factorial(n)
                if n <= 1 then return 1 end
                return n * factorial(n - 1)
            end
            return factorial
        end
        var factorial = makeFactorial()
        global result = factorial(5)
        "#);
    assert_global(&interpreter, "result", Value::Number(120.0));
}

#[test]
fn closures_keep_mutable_state_and_calls_have_independent_scopes() {
    let interpreter = run(r#"
        fn makeCounter()
            var count = 0
            return fn()
                count = count + 1
                return count
            end
        end
        var counter = makeCounter()
        var other = makeCounter()
        global first = counter()
        global second = counter()
        global independent = other()
        global third = counter()
        "#);
    for (name, value) in [
        ("first", 1.0),
        ("second", 2.0),
        ("independent", 1.0),
        ("third", 3.0),
    ] {
        assert_global(&interpreter, name, Value::Number(value));
    }
}

#[test]
fn sibling_closures_share_captured_bindings() {
    let interpreter = run(r#"
        fn makeState()
            var count = 10
            return {
                increment = fn() count = count + 1 end,
                read = fn() return count end
            }
        end
        var state = makeState()
        state.increment()
        state["increment"]()
        global result = state.read()
        "#);
    assert_global(&interpreter, "result", Value::Number(12.0));
}

#[test]
fn closures_capture_parameters_across_multiple_levels() {
    let interpreter = run(r#"
        fn make(a)
            return fn(b)
                return fn(c)
                    a = a + 1
                    b = b + 2
                    return a + b + c
                end
            end
        end
        var closure = make(1)(10)
        global first = closure(100)
        global second = closure(100)
        global immediate = (fn(x) return x * 3 end)(4)
        "#);
    assert_global(&interpreter, "first", Value::Number(114.0));
    assert_global(&interpreter, "second", Value::Number(117.0));
    assert_global(&interpreter, "immediate", Value::Number(12.0));
}

#[test]
fn closures_retain_if_scopes_and_observe_later_assignments() {
    let interpreter = run(r#"
        var closure
        if true then
            var value = 1
            closure = fn() return value end
            value = 7
        end
        global result = closure()
        fn fromBranch()
            if true then
                var local = 12
                return fn() return local end
            end
        end
        global branchResult = fromBranch()()
        "#);
    assert_global(&interpreter, "result", Value::Number(7.0));
    assert_global(&interpreter, "branchResult", Value::Number(12.0));
}

#[test]
fn functions_use_lexical_scope_and_arguments_use_caller_scope() {
    let interpreter = run(r#"
        var value = 10
        fn read() return value end
        fn add(x) return x + value end
        fn caller()
            var value = 99
            return read() + add(value)
        end
        global result = caller()
        global unchanged = value
        "#);
    assert_global(&interpreter, "result", Value::Number(119.0));
    assert_global(&interpreter, "unchanged", Value::Number(10.0));
}

#[test]
fn caller_locals_are_inaccessible_to_callees() {
    for body in ["return secret", "secret = 5"] {
        let mut interpreter = Interpreter::new();
        let source =
            format!("fn callee() {body} end\nfn caller() var secret = 42 callee() end\ncaller()");
        let error = interpreter.run_source(&source).unwrap_err();
        assert!(
            error.message.contains("undeclared variable 'secret'"),
            "{error}"
        );
    }
}

#[test]
fn parameters_and_locals_shadow_outer_names_without_leaking() {
    let mut interpreter = run(r#"
        var value = 10
        fn change(value)
            var local = value + 1
            var value = local
            if true then
                var value = 100
                var branchLocal = value
            end
            return value
        end
        global first = change(4)
        global second = change(7)
        global outside = value
        "#);
    assert_global(&interpreter, "first", Value::Number(5.0));
    assert_global(&interpreter, "second", Value::Number(8.0));
    assert_global(&interpreter, "outside", Value::Number(10.0));
    for name in ["local", "branchLocal"] {
        assert!(interpreter.run_source(&format!("print({name})")).is_err());
    }
}

#[test]
fn callee_then_arguments_are_evaluated_once_left_to_right() {
    let interpreter = run(r#"
        global order = 0
        fn make()
            order = order * 10 + 1
            return fn(a, b) return a * 10 + b end
        end
        fn argument(value)
            order = order * 10 + value
            return value
        end
        global result = make()(argument(2), argument(3))
        "#);
    assert_global(&interpreter, "order", Value::Number(123.0));
    assert_global(&interpreter, "result", Value::Number(23.0));
}

#[test]
fn arity_errors_report_expected_and_actual_counts() {
    for (source, message) in [
        (
            "fn add(a, b) end add(1)",
            "add() expects 2 argument(s), got 1",
        ),
        (
            "fn add(a, b) end add(1, 2, 3)",
            "add() expects 2 argument(s), got 3",
        ),
        (
            "fn empty() end empty(1)",
            "empty() expects 0 argument(s), got 1",
        ),
        (
            "var f = fn(x) end f()",
            "<anonymous>() expects 1 argument(s), got 0",
        ),
        ("str()", "str() expects 1 argument(s), got 0"),
        ("num(1, 2)", "num() expects 1 argument(s), got 2"),
    ] {
        let error = Interpreter::new().run_source(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
    }
}

#[test]
fn arity_checks_happen_after_arguments_but_before_function_body() {
    let mut interpreter = run(r#"
        global argumentsRun = 0
        global bodyRan = false
        fn mark()
            argumentsRun = argumentsRun + 1
            return argumentsRun
        end
        fn one(x) bodyRan = true end
        "#);
    assert!(interpreter.run_source("one(mark(), mark())").is_err());
    assert_global(&interpreter, "argumentsRun", Value::Number(2.0));
    assert_global(&interpreter, "bodyRan", Value::Boolean(false));
}

#[test]
fn argument_and_return_expression_errors_propagate() {
    let mut interpreter = run(r#"
        global touched = false
        fn touch() touched = true return 1 end
        fn accept(a, b) touched = true end
        fn fail() return "text" + 1 end
        "#);
    let error = interpreter
        .run_source("accept(fail(), touch())")
        .unwrap_err();
    assert!(
        error
            .message
            .contains("arithmetic requires Number, got String")
    );
    assert_global(&interpreter, "touched", Value::Boolean(false));
    let error = interpreter.run_source("unknown(touch())").unwrap_err();
    assert!(error.message.contains("undeclared variable 'unknown'"));
    assert_global(&interpreter, "touched", Value::Boolean(false));
}

#[test]
fn runtime_errors_restore_all_caller_and_block_scopes() {
    let mut interpreter = run(r#"
        var visible = "file"
        fn fail(parameter)
            var visible = "callee"
            if true then
                var hidden = 1
                return unknown
            end
        end
        fn caller()
            var visible = "caller"
            fail(1)
        end
        "#);
    let error = interpreter
        .run_source("if true then var visible = 'block' caller() end")
        .unwrap_err();
    assert!(error.message.contains("undeclared variable 'unknown'"));
    interpreter.run_source("global restored = visible").unwrap();
    assert_global(&interpreter, "restored", Value::String("file".into()));
    for name in ["parameter", "hidden"] {
        assert!(interpreter.run_source(&format!("print({name})")).is_err());
    }
}

#[test]
fn calling_non_functions_reports_the_value_type() {
    for (expression, name) in [
        ("42", "Number"),
        ("nil", "Nil"),
        ("true", "Boolean"),
        ("'text'", "String"),
        ("[]", "List"),
        ("{}", "Object"),
    ] {
        let error = Interpreter::new()
            .run_source(&format!("({expression})()"))
            .unwrap_err();
        assert_eq!(
            error.message,
            format!("cannot call {name}; expected Function")
        );
    }
}

#[test]
fn functions_and_builtins_are_first_class_values() {
    let interpreter = run(r#"
        fn apply(f, x) return f(x) end
        var functions = [fn(x) return x + 1 end, num]
        var object = { convert = str }
        global incremented = apply(functions[1], 4)
        global number = functions[2]("12")
        global text = object.convert(12)
        const identity: Function = fn(x) return x end
        global typed = identity(7)
        global nativeType: Function = num
        "#);
    assert_global(&interpreter, "incremented", Value::Number(5.0));
    assert_global(&interpreter, "number", Value::Number(12.0));
    assert_global(&interpreter, "text", Value::String("12".into()));
    assert_global(&interpreter, "typed", Value::Number(7.0));
}

#[test]
fn builtins_obey_normal_shadowing_and_named_declarations_redeclare_vars() {
    let interpreter = run(r#"
        var existing = 1
        fn existing() return 2 end
        fn callLocally()
            fn num(x) return x + 3 end
            return num(4)
        end
        global replaced = existing()
        global local = callLocally()
        global builtin = num("5")
        global str = fn(x) return x + 6 end
        global replacedBuiltin = str(1)
        "#);
    for (name, value) in [
        ("replaced", 2.0),
        ("local", 7.0),
        ("builtin", 5.0),
        ("replacedBuiltin", 7.0),
    ] {
        assert_global(&interpreter, name, Value::Number(value));
    }
}

#[test]
fn function_identity_and_formatting_do_not_traverse_captured_scopes() {
    let interpreter = run(r#"
        fn recursive() return recursive end
        var alias = recursive
        fn make() return fn() end end
        global same = recursive == alias
        global selfReturned = recursive() == recursive
        global distinct = make() ~= make()
        global functions = [recursive, alias]
        global text = str(recursive)
        global truthy = not not recursive
        "#);
    for name in ["same", "selfReturned", "distinct", "truthy"] {
        assert_global(&interpreter, name, Value::Boolean(true));
    }
    assert_global(
        &interpreter,
        "text",
        Value::String("<function recursive>".into()),
    );
    let functions = interpreter.get_global("functions").unwrap();
    assert!(format!("{functions:?}").contains("UserFunction"));
    assert_eq!(
        functions.to_string(),
        "[<function recursive>, <function recursive>]"
    );
}

#[test]
fn globals_remain_shared_with_escaping_closures() {
    let interpreter = run(r#"
        global score = 1
        fn make()
            global created = 5
            return fn()
                score = score + created
                return score
            end
        end
        var closure = make()
        global first = closure()
        created = 10
        global second = closure()
        "#);
    assert_global(&interpreter, "first", Value::Number(6.0));
    assert_global(&interpreter, "second", Value::Number(16.0));
    assert_global(&interpreter, "score", Value::Number(16.0));
}

#[test]
fn captured_constants_and_annotations_remain_enforced() {
    for (declaration, action, expected) in [
        (
            "const value = 1",
            "value = 2",
            "cannot reassign constant 'value'",
        ),
        (
            "const value = 1",
            "value = nil",
            "cannot reassign constant 'value'",
        ),
        (
            "var value: Number = 1",
            "value = 'bad'",
            "type mismatch for 'value': expected Number, got String",
        ),
        (
            "var value: Number = 1",
            "value = nil",
            "type mismatch for 'value': expected Number, got Nil",
        ),
    ] {
        let source = format!(
            "fn make() {declaration} return fn() {action} end end var closure = make() closure()"
        );
        let error = Interpreter::new().run_source(&source).unwrap_err();
        assert_eq!(error.message, expected);
    }
    let error = Interpreter::new()
        .run_source("const fixed = 1 fn fixed() end")
        .unwrap_err();
    assert_eq!(error.message, "cannot redeclare constant 'fixed'");
}

#[test]
fn nil_assignment_removes_captured_local_and_global_bindings() {
    let mut interpreter = run(r#"
        fn make()
            var value = 1
            return {
                remove = fn() value = nil end,
                read = fn() return value end
            }
        end
        var state = make()
        state.remove()
        global removable = 1
        fn removeGlobal() removable = nil end
        removeGlobal()
        "#);
    assert_eq!(interpreter.get_global("removable"), None);
    let error = interpreter.run_source("state.read()").unwrap_err();
    assert!(error.message.contains("undeclared variable 'value'"));
}

#[test]
fn nil_return_assignment_deletes_a_binding_and_unmasks_outer_scope() {
    let interpreter = run(r#"
        var value = 10
        fn empty() end
        fn readOuter()
            var value = 20
            value = empty()
            return value
        end
        global result = readOuter()
        "#);
    assert_global(&interpreter, "result", Value::Number(10.0));
}

#[test]
fn existing_operators_collections_and_if_behavior_work_inside_functions() {
    let interpreter = run(r#"
        fn calculate()
            var numbers: List = [2, 3, 4]
            var object: Object = { total = 0 }
            if false then
                missing()
            elseif 0 and "" and [] then
                var local: Number = numbers[1] + numbers[2] * numbers[3]
                object.total = local
            else
                missing()
            end
            numbers[1] = 5
            object["total"] = object.total + numbers[1]
            return str(object.total) .. ":" .. (#numbers + 2 ^ 3 // 3)
        end
        global result = calculate()
        "#);
    assert_global(&interpreter, "result", Value::String("19:5".into()));
}

#[test]
fn closures_and_ast_bodies_remain_usable_across_source_runs() {
    let mut interpreter =
        run("fn make(x) return fn() x = x + 1 return x end end var counter = make(0)");
    interpreter.run_source("global first = counter()").unwrap();
    interpreter.run_source("global second = counter()").unwrap();
    assert_global(&interpreter, "first", Value::Number(1.0));
    assert_global(&interpreter, "second", Value::Number(2.0));
}

#[test]
fn recursive_calls_retain_distinct_captured_parameters() {
    let interpreter = run(r#"
        var saved = [nil, nil, nil]
        fn collect(n)
            if n == 0 then return end
            saved[n] = fn() return n end
            collect(n - 1)
        end
        collect(3)
        global first = saved[1]()
        global second = saved[2]()
        global third = saved[3]()
        "#);
    for (name, value) in [("first", 1.0), ("second", 2.0), ("third", 3.0)] {
        assert_global(&interpreter, name, Value::Number(value));
    }
}

#[test]
fn mutually_recursive_nested_functions_retain_their_scope() {
    let interpreter = run(r#"
        fn makeEven()
            fn even(n)
                if n == 0 then return true end
                return odd(n - 1)
            end
            fn odd(n)
                if n == 0 then return false end
                return even(n - 1)
            end
            return even
        end
        var even = makeEven()
        global yes = even(10)
        global no = even(9)
        "#);
    assert_global(&interpreter, "yes", Value::Boolean(true));
    assert_global(&interpreter, "no", Value::Boolean(false));
}

#[test]
fn nested_arity_errors_restore_scope_and_preserve_completed_captured_mutations() {
    let mut interpreter = run(r#"
        var visible = "file"
        fn make()
            var count: Number = 0
            fn needsArgument(x) return x end
            fn fail()
                count = count + 1
                var visible = "callee"
                needsArgument()
            end
            return { fail = fail, read = fn() return count end }
        end
        var state = make()
        fn caller()
            var visible = "caller"
            if true then state.fail() end
        end
        "#);
    let error = interpreter.run_source("caller()").unwrap_err();
    assert_eq!(
        error.message,
        "needsArgument() expects 1 argument(s), got 0"
    );
    interpreter
        .run_source("global restored = visible global count = state.read()")
        .unwrap();
    assert_global(&interpreter, "restored", Value::String("file".into()));
    assert_global(&interpreter, "count", Value::Number(1.0));
}

#[test]
fn rebinding_the_callee_during_arguments_does_not_change_the_active_call() {
    let interpreter = run(r#"
        var target = fn(x) return 10 + x end
        fn replace()
            target = fn(x) return 20 + x end
            return 1
        end
        global active = target(replace())
        global subsequent = target(1)
        "#);
    assert_global(&interpreter, "active", Value::Number(11.0));
    assert_global(&interpreter, "subsequent", Value::Number(21.0));
}
