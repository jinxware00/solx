//! Reproducible cycle-allocation smoke profile, not a Lua performance comparison.
use solx::interpreter::Interpreter;
use solx::runtime::collect_garbage;
use std::time::Instant;

fn main() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            "import 'list'
        fn allocate(count)
            for i = 1, count do
                var items = []
                list.append(items, items)
                var object = { items = items }
                var capture = fn() return object end
            end
        end",
        )
        .unwrap();
    let before = collect_garbage();
    let started = Instant::now();
    let mut after = before;
    for _ in 0..25 {
        interpreter.run_source("allocate(1000)").unwrap();
        after = collect_garbage();
        assert_eq!(
            after.remaining, before.remaining,
            "retained graph grew after a batch"
        );
    }
    println!("workload: 25 batches of 1000 cyclic list/object/closure graphs");
    println!("elapsed_ms: {}", started.elapsed().as_millis());
    println!(
        "collection_passes: {}",
        after.collections - before.collections
    );
    println!(
        "reclaimed_nodes: {}",
        after.total_reclaimed - before.total_reclaimed
    );
    println!("retained_nodes_before: {}", before.remaining);
    println!("retained_nodes_after_each_batch: {}", after.remaining);
    println!("node counts are not bytes, RSS, or a cross-language benchmark");
}
