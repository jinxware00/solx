use solx::interpreter::Interpreter;
use solx::runtime::Value;

fn setup() -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'list' import 'object' import 'string' \
             global items = [3, 1, 2] global words = ['a', 'b'] global record = {x = 1}",
        )
        .unwrap();
    interpreter
}

fn while_borrowed(value: &Value, mutable: bool, run: impl FnOnce()) {
    match (value, mutable) {
        (Value::List(items), true) => {
            let _guard = items.borrow_mut();
            run();
        }
        (Value::List(items), false) => {
            let _guard = items.borrow();
            run();
        }
        (Value::Object(properties), true) => {
            let _guard = properties.borrow_mut();
            run();
        }
        (Value::Object(properties), false) => {
            let _guard = properties.borrow();
            run();
        }
        _ => panic!("fixture must be a collection"),
    }
}

macro_rules! read_conflict {
    ($name:ident, $binding:literal, $expression:literal, $operation:literal, $expected:expr) => {
        #[test]
        fn $name() {
            let mut interpreter = setup();
            let value = interpreter.get_global($binding).unwrap();
            let before = value.to_string();
            let source = concat!("global result = ", $expression);
            while_borrowed(&value, true, || {
                let error = interpreter.run_source(source).unwrap_err();
                assert_eq!(
                    error.message,
                    concat!($operation, ": collection is mutably borrowed")
                );
                assert_eq!(error.location.unwrap().span.unwrap().line, 1);
                assert!(interpreter.get_global("result").is_none());
            });
            assert_eq!(value.to_string(), before, "failed access must not mutate");
            interpreter.run_source(source).unwrap();
            assert_eq!(interpreter.get_global("result"), Some($expected));
        }
    };
}

read_conflict!(
    list_length,
    "items",
    "#items",
    "operator '#'",
    Value::Number(3.0)
);
read_conflict!(
    object_length,
    "record",
    "#record",
    "operator '#'",
    Value::Number(1.0)
);
read_conflict!(
    list_index,
    "items",
    "items[1]",
    "list indexing",
    Value::Number(3.0)
);
read_conflict!(
    object_index,
    "record",
    "record['x']",
    "object indexing",
    Value::Number(1.0)
);
read_conflict!(
    object_property,
    "record",
    "record.x",
    "property access",
    Value::Number(1.0)
);
read_conflict!(
    native_list_length,
    "items",
    "list.len(items)",
    "list.len()",
    Value::Number(3.0)
);
read_conflict!(
    native_remove_length_read,
    "items",
    "list.remove(items, 1)",
    "list.remove()",
    Value::Number(3.0)
);
read_conflict!(
    native_list_find,
    "items",
    "list.find(items, 1)",
    "list.find()",
    Value::Number(2.0)
);
read_conflict!(
    native_object_keys,
    "record",
    "object.keys(record)",
    "object.keys()",
    Value::list(vec![Value::String("x".into())])
);
read_conflict!(
    native_object_has,
    "record",
    "object.has(record, 'x')",
    "object.has()",
    Value::Boolean(true)
);
read_conflict!(
    native_string_join,
    "words",
    "string.join(words, ',')",
    "string.join()",
    Value::String("a,b".into())
);

macro_rules! write_conflict {
    ($name:ident, $binding:literal, $source:literal, $operation:literal, $expected:literal) => {
        #[test]
        fn $name() {
            for mutable in [false, true] {
                let mut interpreter = setup();
                let value = interpreter.get_global($binding).unwrap();
                let before = value.to_string();
                while_borrowed(&value, mutable, || {
                    let error = interpreter.run_source($source).unwrap_err();
                    // remove reads length before it requests a mutable borrow.
                    let reason = if mutable && $operation == "list.remove()" {
                        "collection is mutably borrowed"
                    } else {
                        "collection is already borrowed"
                    };
                    assert_eq!(error.message, format!("{}: {reason}", $operation));
                });
                assert_eq!(value.to_string(), before, "failed write must be atomic");
                interpreter.run_source($source).unwrap();
                assert_eq!(value.to_string(), $expected);
            }
        }
    };
}

write_conflict!(
    list_assignment,
    "items",
    "items[1] = 9",
    "list assignment",
    "[9, 1, 2]"
);
write_conflict!(
    object_assignment,
    "record",
    "record['x'] = 9",
    "object assignment",
    "{x = 9}"
);
write_conflict!(
    property_assignment,
    "record",
    "record.x = 9",
    "property assignment",
    "{x = 9}"
);
write_conflict!(
    native_append,
    "items",
    "list.append(items, 9)",
    "list.append()",
    "[3, 1, 2, 9]"
);
write_conflict!(
    native_remove,
    "items",
    "list.remove(items, 1)",
    "list.remove()",
    "[1, 2]"
);
write_conflict!(
    native_sort,
    "items",
    "list.sort(items)",
    "list.sort()",
    "[1, 2, 3]"
);
write_conflict!(
    native_object_remove,
    "record",
    "object.remove(record, 'x')",
    "object.remove()",
    "{}"
);

#[test]
fn conflict_errors_restore_nested_calls_and_loop_scopes() {
    let mut interpreter = setup();
    interpreter.run_source("var marker = 7\nfn inner()\n  items[1] = 9\nend\nfn outer()\n  var marker = 99\n  for i = 1, 2 do inner() end\nend").unwrap();
    let items = interpreter.get_global("items").unwrap();
    while_borrowed(&items, false, || {
        let error = interpreter.run_source("outer()").unwrap_err();
        assert_eq!(
            error.message,
            "list assignment: collection is already borrowed"
        );
        let span = error.location.unwrap().span.unwrap();
        assert_eq!((span.line, span.column), (3, 3));
        assert_eq!(error.contexts.len(), 2);
        assert_eq!(error.contexts[0].description, "in inner()");
        assert_eq!(error.contexts[1].description, "in outer()");
    });
    interpreter
        .run_source("global restored = marker outer() inner = nil outer = nil")
        .unwrap();
    assert_eq!(interpreter.get_global("restored"), Some(Value::Number(7.0)));
    assert_eq!(items.to_string(), "[9, 1, 2]");
}

#[test]
fn shared_borrows_allow_reads_and_validation_errors_stay_specific() {
    let mut interpreter = setup();
    let items = interpreter.get_global("items").unwrap();
    let record = interpreter.get_global("record").unwrap();
    while_borrowed(&items, false, || {
        while_borrowed(&record, false, || {
            interpreter.run_source("global result = [#items, items[1], list.len(items), list.find(items, 1), #record, record.x, record['x'], object.has(record, 'x')]").unwrap();
            assert_eq!(
                interpreter.get_global("result").unwrap().to_string(),
                "[3, 3, 3, 2, 1, 1, 1, true]"
            );
            assert_eq!(
                interpreter
                    .run_source("list.remove(items, 0)")
                    .unwrap_err()
                    .message,
                "list.remove() argument 2 must be an integer in 1..3"
            );
        });
    });
    while_borrowed(&items, true, || {
        // Invalid read indices are diagnosed without needing the payload.
        interpreter.run_source("global missing = items[0]").unwrap();
        assert_eq!(interpreter.get_global("missing"), Some(Value::Nil));
        assert_eq!(
            interpreter.run_source("items[0] = 9").unwrap_err().message,
            "list index must be a positive integer"
        );
        assert_eq!(
            interpreter
                .run_source("list.append(items)")
                .unwrap_err()
                .message,
            "list.append() expects 2 argument(s), got 1"
        );
    });
}

#[test]
fn ordinary_cyclic_mutation_and_search_do_not_create_borrow_conflicts() {
    let mut interpreter = setup();
    interpreter.run_source("global cycle = [] list.append(cycle, cycle) global found = list.find(cycle, cycle) global text = str(cycle) list.remove(cycle, 1)").unwrap();
    assert_eq!(interpreter.get_global("found"), Some(Value::Number(1.0)));
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("[<cycle>]".into()))
    );
    assert_eq!(interpreter.get_global("cycle"), Some(Value::list(vec![])));
}
