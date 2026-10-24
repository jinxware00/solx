use solx::interpreter::Interpreter;
use solx::runtime::{Value, collect_garbage};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "solx-gc-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        assert!(
            self.0
                .starts_with(std::env::temp_dir().canonicalize().unwrap())
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn aliased_exports_and_host_namespace_survive_initialization_and_interpreter_drop() {
    let project = Project::new();
    project.write(
        "module.solx",
        "import 'list'
        exports.exposed = exports
        exports.state = [42]
        exports.read = fn() return exports.state[1] end
        for i = 1, 900 do var c = [] list.append(c, c) end",
    );
    let entry = project.write(
        "entry.solx",
        "import 'core' import './module.solx' as m
        global namespace = m global exposed = m.exposed
        core.assert(m.read() == 42)",
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_file(entry).unwrap();
    let namespace = interpreter.get_global("namespace").unwrap();
    let exposed = interpreter.get_global("exposed").unwrap();
    let Value::Object(exports) = &exposed else {
        panic!()
    };
    let weak = Rc::downgrade(exports);
    drop(exposed);
    interpreter
        .run_source("namespace = nil exposed = nil")
        .unwrap();
    collect_garbage();
    interpreter
        .run_source("import './module.solx' as again core.assert(again.read() == 42)")
        .unwrap();
    drop(interpreter);
    collect_garbage();
    {
        let exports = weak
            .upgrade()
            .expect("host namespace roots the export graph");
        assert_eq!(exports.borrow()["state"].to_string(), "[42]");
    }
    drop(namespace);
    collect_garbage();
    assert!(weak.upgrade().is_none());
}

#[test]
fn module_cache_is_a_root_even_after_all_import_bindings_leave_scope() {
    let project = Project::new();
    project.write("module.solx", "exports.state = { count = 0 }
        exports.next = fn() exports.state.count = exports.state.count + 1 return exports.state.count end");
    let entry = project.write(
        "entry.solx",
        "try import './module.solx' as m
        global state = m.state m.next() catch err end",
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_file(entry).unwrap();
    let state_value = interpreter.get_global("state").unwrap();
    let Value::Object(state) = &state_value else {
        panic!()
    };
    let weak = Rc::downgrade(state);
    drop(state_value);
    interpreter.run_source("state = nil").unwrap();
    collect_garbage();
    assert!(weak.upgrade().is_some());
    interpreter
        .run_source("import 'core' import './module.solx' as m core.assert(m.next() == 2)")
        .unwrap();
    drop(interpreter);
    assert!(weak.upgrade().is_none());
}

#[test]
fn failed_module_escapes_stay_live_and_overwritten_closures_are_collected() {
    let project = Project::new();
    project.write("store.solx", "exports.state = { count = 0 }");
    project.write(
        "failed.solx",
        "import './store.solx' as store import 'core' import 'list'
        store.state.count = store.state.count + 1
        var own = store.state.count
        store.state.saved = fn() return own end
        for i = 1, 700 do var c = [] list.append(c, c) end
        core.error('expected failure')",
    );
    let entry = project.write(
        "entry.solx",
        "import 'core' import './store.solx' as store
        try import './failed.solx' catch err core.assert(err.message == 'expected failure') end
        global previous = store.state.saved
        core.assert(previous() == 1)",
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_file(entry).unwrap();
    let previous_value = interpreter.get_global("previous").unwrap();
    let Value::Function(previous) = &previous_value else {
        panic!()
    };
    let weak = Rc::downgrade(previous);
    drop(previous_value);
    collect_garbage();
    interpreter
        .run_source(
            "try import './failed.solx' catch err end
        core.assert(previous() == 1) core.assert(store.state.saved() == 2)
        previous = nil",
        )
        .unwrap();
    assert!(weak.upgrade().is_none());
    interpreter
        .run_source("core.assert(store.state.saved() == 2)")
        .unwrap();
}
