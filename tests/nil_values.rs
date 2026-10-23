use solx::interpreter::Interpreter;
use solx::runtime::Value;
use std::process::Command;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(source).unwrap();
    interpreter
}

#[test]
fn nil_declarations_redeclarations_and_assignments_keep_bindings() {
    let mut interpreter = run("var local
         global omitted
         global explicit = nil
         const fixed = nil
         local = nil
         global fromLocal = local
         omitted = nil
         explicit = 42
         explicit = nil
         var local = 1
         var local = nil
         global redeclared = local
         global fromConstant = fixed");
    for name in [
        "omitted",
        "explicit",
        "fromLocal",
        "redeclared",
        "fromConstant",
    ] {
        assert_eq!(interpreter.get_global(name), Some(Value::Nil), "{name}");
    }
    interpreter
        .run_source("local = 7 omitted = local explicit = 8")
        .unwrap();
    assert_eq!(interpreter.get_global("omitted"), Some(Value::Number(7.0)));
    assert_eq!(interpreter.get_global("explicit"), Some(Value::Number(8.0)));
}

#[test]
fn optional_return_values_can_replace_existing_values() {
    let interpreter = run("import 'core' import 'string' import 'list'
         fn optional(found) if found then return 42 end return nil end
         fn implicit() end
         fn bare() return end
         fn identity(value) value = nil return value end
         global explicitResult = optional(true)
         explicitResult = optional(false)
         global implicitResult = 1
         implicitResult = implicit()
         global bareResult = 1
         bareResult = bare()
         global parameterResult = identity(42)
         var found = string.find('abc', 'b')
         core.assert(found == 2)
         found = string.find('abc', 'missing')
         global stringResult = found
         found = list.find([10], 10)
         core.assert(found == 1)
         found = list.find([10], 20)
         global listResult = found");
    for name in [
        "explicitResult",
        "implicitResult",
        "bareResult",
        "parameterResult",
        "stringResult",
        "listResult",
    ] {
        assert_eq!(interpreter.get_global(name), Some(Value::Nil), "{name}");
    }
}

#[test]
fn nil_annotations_accept_nil_and_reject_other_values_without_losing_the_binding() {
    let mut interpreter = run("global value: Nil = nil value = nil value = nil");
    assert_eq!(interpreter.get_global("value"), Some(Value::Nil));
    let error = interpreter.run_source("value = 1").unwrap_err();
    assert_eq!(
        error.message,
        "type mismatch for 'value': expected Nil, got Number"
    );
    assert_eq!(interpreter.get_global("value"), Some(Value::Nil));
    interpreter.run_source("value = nil").unwrap();
    let error = interpreter
        .run_source("global invalid: Nil = false")
        .unwrap_err();
    assert_eq!(
        error.message,
        "type mismatch for 'invalid': expected Nil, got Boolean"
    );
    assert_eq!(interpreter.get_global("invalid"), None);
}

#[test]
fn other_annotations_still_reject_nil_and_preserve_existing_values() {
    for (annotation, expression) in [
        ("Number", "1"),
        ("String", "'text'"),
        ("Boolean", "true"),
        ("List", "[]"),
        ("Object", "{}"),
        ("Module", "core"),
        ("Function", "fn() end"),
    ] {
        let mut interpreter = run(&format!(
            "import 'core' global value: {annotation} = {expression}"
        ));
        let previous = interpreter.get_global("value");
        let error = interpreter.run_source("value = nil").unwrap_err();
        assert_eq!(
            error.message,
            format!("type mismatch for 'value': expected {annotation}, got Nil")
        );
        assert_eq!(interpreter.get_global("value"), previous);
        let error = interpreter
            .run_source(&format!("global invalid: {annotation} = nil"))
            .unwrap_err();
        assert_eq!(
            error.message,
            format!("type mismatch for 'invalid': expected {annotation}, got Nil")
        );
        assert_eq!(interpreter.get_global("invalid"), None);
    }
}

#[test]
fn constants_holding_nil_remain_protected_from_assignment_and_redeclaration() {
    let mut interpreter = run("const value: Nil = nil");
    for (source, expected) in [
        ("value = nil", "cannot reassign constant 'value'"),
        ("value = 1", "cannot reassign constant 'value'"),
        ("var value = nil", "cannot redeclare constant 'value'"),
    ] {
        let error = interpreter.run_source(source).unwrap_err();
        assert_eq!(error.message, expected);
        interpreter.run_source("global retained = value").unwrap();
        assert_eq!(interpreter.get_global("retained"), Some(Value::Nil));
    }
}

#[test]
fn stored_nil_keeps_truthiness_equality_and_conversion_rules() {
    let interpreter = run("import 'core'
         var value = true
         value = nil
         core.assert(not value)
         core.assert(value == nil)
         core.assert(value ~= false)
         core.assert((value and 'unreachable') == nil)
         core.assert((value or 'fallback') == 'fallback')
         core.assert(core.type(value) == 'Nil')
         core.assert(str(value) == 'nil')
         global reached = false
         if value then core.error('nil must remain false') else reached = true end
         while value do core.error('nil must remain false') end");
    assert_eq!(
        interpreter.get_global("reached"),
        Some(Value::Boolean(true))
    );
}

#[test]
fn nil_collection_entries_remain_present_until_explicitly_removed() {
    let interpreter = run("import 'core' import 'list' import 'object'
         var value = 1
         value = nil
         global items = [1, nil, 3]
         items[1] = value
         list.append(items, value)
         core.assert(list.len(items) == 4)
         core.assert(items == [nil, nil, 3, nil])
         core.assert(list.find(items, value) == 1)
         global removedItem = 42
         removedItem = list.remove(items, 1)
         core.assert(items == [nil, 3, nil])
         global record = { first = 1, second = nil }
         record.first = value
         record['third'] = value
         core.assert(object.has(record, 'first'))
         core.assert(object.has(record, 'second'))
         core.assert(object.has(record, 'third'))
         core.assert(object.keys(record) == ['first', 'second', 'third'])
         global removedProperty = 42
         removedProperty = object.remove(record, 'first')
         core.assert(not object.has(record, 'first'))
         core.assert(object.has(record, 'second'))
         global absentProperty = 42
         absentProperty = object.remove(record, 'absent')");
    for name in ["removedItem", "removedProperty", "absentProperty"] {
        assert_eq!(interpreter.get_global(name), Some(Value::Nil), "{name}");
    }
}

#[test]
fn missing_collection_reads_warn_but_their_nil_results_can_be_stored() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args([
            "-e",
            "var value = 1
             var record = { present = nil }
             value = record.absent print(value)
             value = record['other'] print(value)
             var items = [nil]
             value = items[1] print(value)
             value = items[2] print(value)
             value = items[0] print(value)
             print(record.present)
             value = 42 print(value)",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "nil\nnil\nnil\nnil\nnil\nnil\n42\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "Warning: missing object property 'absent'\nWarning: missing object property 'other'\nWarning: list index 2 is out of range\nWarning: invalid list index\n"
    );
}

#[test]
fn undeclared_names_and_expired_locals_still_warn_and_error() {
    for (source, message) in [
        ("print(missing)", "undeclared variable 'missing'"),
        (
            "missing = nil",
            "cannot assign to undeclared variable 'missing'",
        ),
        (
            "if true then var missing = nil end print(missing)",
            "undeclared variable 'missing'",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(["-e", source])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Warning: undeclared variable 'missing'"));
        assert!(stderr.contains(&format!("solx: {message}")));
    }
}
