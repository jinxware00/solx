//! Deterministic cross-layer checks supplement individual feature examples.
use solx::{interpreter::Interpreter, runtime::Value};

#[test]
fn generated_integer_arithmetic_matches_wider_reference_and_recovers_after_errors() {
    let mut interpreter = Interpreter::new();
    let mut state = 0x73f1_963d_093b_52a7_u64;
    for _ in 0..192 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let a = state as i64;
        let b = state.rotate_left(31) as i64;
        for (op, reference) in [
            ("+", a as i128 + b as i128),
            ("-", a as i128 - b as i128),
            ("*", a as i128 * b as i128),
        ] {
            interpreter.run_source("global answer = nil").unwrap();
            let result = interpreter.run_source(&format!("answer = ({a}) {op} ({b})"));
            if let Ok(expected) = i64::try_from(reference) {
                result.unwrap();
                assert_eq!(
                    interpreter.get_global("answer"),
                    Some(Value::Integer(expected))
                );
            } else {
                let error = result.unwrap_err();
                assert!(error.is_catchable());
                assert!(error.message.contains("overflow"));
                assert_eq!(interpreter.get_global("answer"), Some(Value::Nil));
            }
        }
    }
    interpreter.run_source("answer = 42").unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Integer(42)));
}

#[test]
fn generated_json_nil_iteration_callbacks_and_closures_agree() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("import 'core' import 'json' import 'list' import 'object'")
        .unwrap();
    for size in 0..32 {
        let values: Vec<_> = (0..size)
            .map(|index| {
                if index % 3 == 0 {
                    "null".to_owned()
                } else {
                    (9_007_199_254_740_992_i64 + index).to_string()
                }
            })
            .collect();
        let json = format!("[{}]", values.join(","));
        interpreter
            .run_source(&format!(
                r#"
            var items = json.parse('{json}')
            var saved = list.map(items, fn(value) return value end)
            var readers = []
            for index, value in items do
                list.append(readers, fn() return value end)
                items[index] = nil
            end
            core.assert(#items == {size})
            core.assert(json.stringify(saved) == '{json}')
            for index, reader in readers do core.assert(reader() == saved[index]) end
            var record = {{absent = nil, entries = saved}}
            var restored = json.parse(json.stringify(record))
            core.assert(object.has(restored, 'absent'))
            core.assert(restored == record)
            var copied = list.fold(saved, [], fn(acc, value) list.append(acc, value) return acc end)
            core.assert(copied == saved)
        "#
            ))
            .unwrap();
    }
}

#[test]
fn generated_stable_sort_preserves_original_position_for_equal_keys() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
        import 'core' import 'list'
        var records = []
        for i = 1, 200 do list.append(records, {key = (i * 73) % 11, original = i}) end
        var result = list.sorted(records, fn(a, b) return a.key - b.key end)
        core.assert(#result == #records)
        for i = 2, #result do
            core.assert(result[i-1].key <= result[i].key)
            if result[i-1].key == result[i].key then
                core.assert(result[i-1].original < result[i].original)
            end
        end
        for i, value in records do core.assert(value.original == i) end
        global recovered = false
        try list.sorted(records, fn(a, b) return nil end)
        catch error recovered = true end
        core.assert(recovered)
        for i, value in records do core.assert(value.original == i) end
    "#,
        )
        .unwrap();
}

#[test]
fn unicode_scalars_round_trip_through_strings_json_and_raw_literals() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
        import 'core' import 'string' import 'json'
        var scalars = [0, 10, 13, 34, 92, 127, 233, 769, 65535, 128578, 1114111]
        var text = string.from_codepoints(scalars)
        core.assert(string.codepoints(text) == scalars)
        core.assert(json.parse(json.stringify(text)) == text)
        core.assert(string.len(text) == #scalars)
        core.assert(string.byte_len(text) > #scalars)
        var raw = """a\n{literal}
b"""
        core.assert(json.parse(json.stringify(raw)) == raw)
        var caught = 0
        for scalar in [-1, 55296, 57343, 1114112, 1.5] do
            try string.from_codepoints([scalar]) catch error caught = caught + 1 end
        end
        core.assert(caught == 5)
        core.assert(string.replace('aaaaa', 'aa', 'x') == 'xxa')
        try string.replace('ok', '', 'bad') catch error caught = caught + 1 end
        core.assert(caught == 6)
    "#,
        )
        .unwrap();
}
