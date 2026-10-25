//! Development-only phase timings; no timing assertions or cross-runtime claims.
use solx::{
    interpreter::Interpreter,
    parser::parse,
    runtime::{Value, collect_garbage},
};
use std::time::Instant;

fn distribution(samples: &[u128]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    serde_json::json!({
        "samples_ns": samples,
        "median_ns": sorted[sorted.len() / 2],
        "p95_ns": sorted[(sorted.len() * 95).div_ceil(100) - 1],
        "maximum_ns": sorted.last(),
    })
}

fn main() {
    const SOURCE: &str = "global sum = 0 for i = 1, 10000 do sum = sum + i end";
    const SAMPLES: usize = 21;
    const WARMUPS: usize = 2;
    const NODES: usize = 2_000;
    let mut parsing = Vec::new();
    let mut execution = Vec::new();
    let mut allocation = Vec::new();
    let mut collection = Vec::new();
    let mut reclaimed = Vec::new();
    for trial in 0..SAMPLES + WARMUPS {
        let started = Instant::now();
        let program = parse(SOURCE).unwrap();
        let parse_ns = started.elapsed().as_nanos();
        let mut interpreter = Interpreter::new();
        let started = Instant::now();
        interpreter.run_program(&program).unwrap();
        let execution_ns = started.elapsed().as_nanos();
        assert_eq!(
            interpreter.get_global("sum"),
            Some(Value::Integer(50_005_000))
        );
        drop(interpreter);
        let before = collect_garbage();
        let started = Instant::now();
        for _ in 0..NODES {
            let value = Value::list(vec![]);
            if let Value::List(items) = &value {
                items.borrow_mut().push(value.clone());
            }
        }
        let allocation_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let after = collect_garbage();
        let collection_ns = started.elapsed().as_nanos();
        assert_eq!(after.remaining, before.remaining);
        assert_eq!(after.reclaimed, NODES);
        if trial >= WARMUPS {
            parsing.push(parse_ns);
            execution.push(execution_ns);
            allocation.push(allocation_ns);
            collection.push(collection_ns);
            reclaimed.push(after.reclaimed);
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "schemaVersion": 1,
            "samples": SAMPLES,
            "warmups": WARMUPS,
            "source": SOURCE,
            "parse": distribution(&parsing),
            "preparsed_execution": distribution(&execution),
            "cyclic_graph_allocation": distribution(&allocation),
            "explicit_collection_pause": distribution(&collection),
            "graphs_per_sample": NODES,
            "reclaimed_nodes": reclaimed,
            "limitations": [
                "Execution includes automatic collection and AST validation, excludes interpreter construction/destruction.",
                "Allocation timings create one self-cyclic list per graph; these are graph counts, not allocator calls or bytes.",
                "Collection samples measure full synchronous explicit passes, not all automatic GC pauses.",
                "Peak process memory requires an external OS measurement; Rust does not expose portable RSS."
            ]
        })
    );
}
