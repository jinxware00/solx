use std::collections::HashMap;

use solx::interpreter::Interpreter;
use solx::runtime::Value;

fn clear(value: &Value) {
    match value {
        Value::List(items) => items.borrow_mut().clear(),
        Value::Object(properties) => properties.borrow_mut().clear(),
        _ => {}
    }
}

fn self_list(tail: Value) -> Value {
    let value = Value::list(vec![]);
    if let Value::List(items) = &value {
        items.borrow_mut().extend([value.clone(), tail]);
    }
    value
}

#[test]
fn direct_list_cycles_format_debug_and_compare() {
    let a = self_list(Value::Number(1.0));
    let b = self_list(Value::Number(1.0));
    let different = self_list(Value::Number(2.0));
    assert_eq!(a.to_string(), "[<cycle>, 1]");
    assert_eq!(format!("{a:?}"), "[<cycle>, Number(1.0)]");
    assert_eq!(a, a);
    assert_eq!(a, b);
    assert_ne!(a, different);
    for value in [&a, &b, &different] {
        clear(value);
    }
}

#[test]
fn direct_objects_and_indirect_mixed_cycles_are_safe() {
    let object = Value::object(HashMap::new());
    let list = Value::list(vec![object.clone()]);
    if let Value::Object(properties) = &object {
        properties
            .borrow_mut()
            .insert("self".into(), object.clone());
        properties.borrow_mut().insert("list".into(), list.clone());
    }
    assert_eq!(object.to_string(), "{list = [<cycle>], self = <cycle>}");
    assert!(format!("{object:#?}").contains("<cycle>"));
    assert_eq!(object, object);
    assert_ne!(object, list);
    clear(&object);
    clear(&list);
}

#[test]
fn cyclic_equality_compares_content_not_graph_topology() {
    let one = Value::list(vec![]);
    let two_a = Value::list(vec![]);
    let two_b = Value::list(vec![two_a.clone()]);
    if let (Value::List(a), Value::List(b)) = (&one, &two_a) {
        a.borrow_mut().push(one.clone());
        b.borrow_mut().push(two_b.clone());
    }
    assert_eq!(one, two_a);
    assert_eq!(two_a.to_string(), "[[<cycle>]]");
    for value in [&one, &two_a, &two_b] {
        clear(value);
    }
}

#[test]
fn independently_allocated_object_cycles_compare_all_keys_and_scalar_leaves() {
    fn cycle(number: f64) -> Value {
        let value = Value::object(HashMap::new());
        if let Value::Object(properties) = &value {
            properties.borrow_mut().extend([
                ("self".into(), value.clone()),
                ("value".into(), Value::Number(number)),
            ]);
        }
        value
    }
    let a = cycle(1.0);
    let b = cycle(1.0);
    let different = cycle(2.0);
    let nan = cycle(f64::NAN);
    assert_eq!(a, b);
    assert_ne!(a, different);
    assert_ne!(nan, nan);
    if let Value::Object(properties) = &b {
        let value = properties.borrow_mut().remove("value").unwrap();
        properties.borrow_mut().insert("other".into(), value);
    }
    assert_ne!(a, b);
    for value in [&a, &b, &different, &nan] {
        clear(value);
    }
}

#[test]
fn acyclic_sharing_and_scalar_semantics_are_preserved() {
    let shared = Value::list(vec![Value::String("hello".into()), Value::Nil]);
    let outer = Value::list(vec![shared.clone(), shared]);
    assert_eq!(outer.to_string(), "[[hello, nil], [hello, nil]]");
    let nan = Value::list(vec![Value::Number(f64::NAN)]);
    assert_ne!(nan, nan);
    let object = Value::object(HashMap::from([
        ("a".into(), Value::Number(1.0)),
        ("a ".into(), Value::Number(2.0)),
    ]));
    assert_eq!(object.to_string(), "{a  = 2, a = 1}");
    assert_eq!(Value::list(vec![]).to_string(), "[]");
    assert_eq!(Value::object(HashMap::new()).to_string(), "{}");
    assert_ne!(
        object,
        Value::object(HashMap::from([("b".into(), Value::Number(1.0))]))
    );
}

#[test]
fn deep_acyclic_traversal_uses_work_stacks() {
    let mut nodes = vec![Value::list(vec![Value::Number(1.0)])];
    for _ in 0..2_000 {
        nodes.push(Value::list(vec![nodes.last().unwrap().clone()]));
    }
    let root = nodes.last().unwrap();
    assert_eq!(root.to_string().len(), 4_003);
    assert!(format!("{root:?}").contains("Number(1.0)"));
    assert_eq!(root, root);
    // Destruction is a separate operation: break links while every node is held.
    for node in &nodes {
        clear(node);
    }
}

#[test]
fn debugging_a_host_borrowed_collection_does_not_panic() {
    let value = Value::list(vec![]);
    if let Value::List(items) = &value {
        let _borrow = items.borrow_mut();
        assert_eq!(format!("{value:?}"), "<borrowed>");
    }
}

#[test]
fn solx_conversion_concatenation_equality_and_search_accept_cycles() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
        import "list"
        import "object"
        var a = []
        var b = []
        list.append(a, a)
        list.append(b, b)
        global text = str(a) .. "!"
        global same = a == b
        global different = a ~= b
        global found = list.find([a], b)
        var o = {}
        o.self = o
        global objectText = str(o)
        print(a, o)
        list.remove(a, 1)
        list.remove(b, 1)
        object.remove(o, "self")
    "#,
        )
        .unwrap();
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("[<cycle>]!".into()))
    );
    assert_eq!(
        interpreter.get_global("objectText"),
        Some(Value::String("{self = <cycle>}".into()))
    );
    assert_eq!(interpreter.get_global("same"), Some(Value::Boolean(true)));
    assert_eq!(
        interpreter.get_global("different"),
        Some(Value::Boolean(false))
    );
    assert_eq!(interpreter.get_global("found"), Some(Value::Number(1.0)));
}

#[test]
fn equality_with_a_mutably_borrowed_list_is_false_and_recovers() {
    let a = Value::list(vec![Value::Number(1.0)]);
    let alias = a.clone();
    let b = Value::list(vec![Value::Number(1.0)]);
    let (Value::List(items), Value::List(other)) = (&a, &b) else {
        unreachable!()
    };
    let mut guard = items.borrow_mut();
    assert_ne!(a, alias);
    assert_ne!(a, b);
    assert_ne!(b, a);
    {
        let _other_guard = other.borrow_mut();
        assert_ne!(a, b);
        assert_ne!(b, a);
    }
    guard[0] = Value::Number(2.0);
    drop(guard);
    assert_eq!(a, alias);
    assert_ne!(a, b);
    other.borrow_mut()[0] = Value::Number(2.0);
    assert_eq!(a, b);
}

#[test]
fn equality_with_a_mutably_borrowed_object_is_false_and_recovers() {
    let a = Value::object(HashMap::from([("n".into(), Value::Number(1.0))]));
    let alias = a.clone();
    let b = Value::object(HashMap::from([("n".into(), Value::Number(1.0))]));
    let (Value::Object(properties), Value::Object(other)) = (&a, &b) else {
        unreachable!()
    };
    let mut guard = properties.borrow_mut();
    assert_ne!(a, alias);
    assert_ne!(a, b);
    assert_ne!(b, a);
    {
        let _other_guard = other.borrow_mut();
        assert_ne!(a, b);
        assert_ne!(b, a);
    }
    guard.insert("n".into(), Value::Number(2.0));
    drop(guard);
    assert_eq!(a, alias);
    assert_ne!(a, b);
    other.borrow_mut().insert("n".into(), Value::Number(2.0));
    assert_eq!(a, b);
}

fn mixed_cycle() -> (Value, Value) {
    let object = Value::object(HashMap::new());
    let list = Value::list(vec![object.clone()]);
    if let Value::Object(properties) = &object {
        properties.borrow_mut().insert("items".into(), list.clone());
    }
    (object, list)
}

#[test]
fn nested_borrow_conflicts_in_mixed_cycles_do_not_panic_or_cache_inequality() {
    let (a, a_list) = mixed_cycle();
    let (b, b_list) = mixed_cycle();
    let (Value::Object(properties), Value::List(items)) = (&a, &a_list) else {
        unreachable!()
    };
    {
        let _guard = properties.borrow_mut();
        assert_ne!(a_list, b_list);
        assert_ne!(b_list, a_list);
        assert_ne!(a_list, a_list.clone());
    }
    assert_eq!(a_list, b_list);
    {
        let _guard = items.borrow_mut();
        assert_ne!(a, b);
        assert_ne!(b, a);
        assert_ne!(a, a.clone());
    }
    assert_eq!(a, b);
    assert_eq!(a, a.clone());
    for value in [&a, &a_list, &b, &b_list] {
        clear(value);
    }
}

#[test]
fn formatting_and_debugging_mixed_cycles_survive_host_borrows() {
    let (object, list) = mixed_cycle();
    let (Value::Object(properties), Value::List(items)) = (&object, &list) else {
        unreachable!()
    };
    {
        let _guard = properties.borrow_mut();
        assert_eq!(object.to_string(), "<borrowed>");
        assert_eq!(format!("{object:?}"), "<borrowed>");
        assert_eq!(list.to_string(), "[<borrowed>]");
        assert_eq!(format!("{list:?}"), "[<borrowed>]");
    }
    {
        let _guard = items.borrow_mut();
        assert_eq!(list.to_solx_string(), "<borrowed>");
        assert_eq!(format!("{list:#?}"), "<borrowed>");
        assert_eq!(object.to_string(), "{items = <borrowed>}");
        assert_eq!(format!("{object:?}"), r#"{"items" = <borrowed>}"#);
    }
    assert_eq!(object.to_string(), "{items = [<cycle>]}");
    assert_eq!(format!("{object:?}"), r#"{"items" = [<cycle>]}"#);
    assert_eq!(list.to_string(), "[{items = <cycle>}]");
    assert_eq!(format!("{list:#?}"), r#"[{"items" = <cycle>}]"#);
    clear(&object);
    clear(&list);
}

#[test]
fn acyclic_equality_under_shared_borrows_preserves_structure_and_nan() {
    let leaf = Value::list(vec![Value::Number(1.0), Value::String("x".into())]);
    let a = Value::object(HashMap::from([
        (
            "items".into(),
            Value::list(vec![leaf.clone(), leaf.clone()]),
        ),
        ("optional".into(), Value::Nil),
    ]));
    let b = Value::object(HashMap::from([
        ("optional".into(), Value::Nil),
        (
            "items".into(),
            Value::list(vec![
                Value::list(vec![Value::Number(1.0), Value::String("x".into())]),
                Value::list(vec![Value::Number(1.0), Value::String("x".into())]),
            ]),
        ),
    ]));
    let (Value::Object(properties), Value::List(items)) = (&a, &leaf) else {
        unreachable!()
    };
    {
        let _object_guard = properties.borrow();
        let _list_guard = items.borrow();
        assert_eq!(a, b);
        assert_eq!(b, a);
        assert_eq!(a, a.clone());
    }
    items.borrow_mut()[0] = Value::Number(f64::NAN);
    assert_ne!(a, a.clone());
    assert_ne!(a, b);
    items.borrow_mut()[0] = Value::Number(1.0);
    assert_eq!(a, b);
}

#[test]
fn host_borrowed_equality_and_search_preserve_runtime_errors_and_recover() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("import 'list' global a = [1] global b = [1] global haystack = [a]")
        .unwrap();
    interpreter
        .run_source("fn fail()\n  return a == num('bad')\nend")
        .unwrap();
    let Value::List(ref items) = interpreter.get_global("a").unwrap() else {
        unreachable!()
    };
    let guard = items.borrow_mut();
    let comparisons = "global same = a == b global different = a ~= b global alias = a == a global found = list.find(haystack, b)";
    interpreter.run_source(comparisons).unwrap();
    assert_eq!(interpreter.get_global("same"), Some(Value::Boolean(false)));
    assert_eq!(
        interpreter.get_global("different"),
        Some(Value::Boolean(true))
    );
    assert_eq!(interpreter.get_global("alias"), Some(Value::Boolean(false)));
    assert_eq!(interpreter.get_global("found"), Some(Value::Nil));

    let error = interpreter.run_source("fail()").unwrap_err();
    assert_eq!(error.message, "cannot convert 'bad' to Number");
    let span = error.location.unwrap().span.unwrap();
    assert_eq!((span.line, span.column), (2, 3));
    assert_eq!(error.contexts.len(), 1);
    assert_eq!(error.contexts[0].description, "in fail()");
    assert_eq!(
        interpreter
            .run_source("list.find(1, a)")
            .unwrap_err()
            .message,
        "list.find() argument 1 requires List, got Number"
    );
    assert_eq!(guard.len(), 1);
    drop(guard);

    interpreter.run_source(comparisons).unwrap();
    assert_eq!(interpreter.get_global("same"), Some(Value::Boolean(true)));
    assert_eq!(
        interpreter.get_global("different"),
        Some(Value::Boolean(false))
    );
    assert_eq!(interpreter.get_global("alias"), Some(Value::Boolean(true)));
    assert_eq!(interpreter.get_global("found"), Some(Value::Number(1.0)));
    interpreter.run_source("fail = nil").unwrap();
}
