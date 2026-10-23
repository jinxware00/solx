//! Source identities, path resolution, loading state, and per-interpreter cache.
//! Source execution stays in the interpreter; no evaluator logic lives here.
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::runtime::{RuntimeError, Value};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ModuleId {
    Builtin(String),
    File(PathBuf),
}

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(name) => write!(f, "{name}"),
            Self::File(path) => write!(f, "{}", path.display()),
        }
    }
}

pub(crate) struct ModuleLoader {
    base_directory: Option<PathBuf>,
    cache: HashMap<ModuleId, Value>,
    loading: Vec<ModuleId>,
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self {
            base_directory: std::env::current_dir().ok(),
            cache: HashMap::new(),
            loading: Vec::new(),
        }
    }
}

impl ModuleLoader {
    pub(crate) fn resolve(
        &self,
        specifier: &str,
        source_file: Option<&Path>,
    ) -> Result<ModuleId, RuntimeError> {
        let path = Path::new(specifier);
        if !specifier.contains(['/', '\\', '.']) {
            return Ok(ModuleId::Builtin(specifier.to_owned()));
        }
        if path.extension() != Some(OsStr::new("solx")) {
            return Err(RuntimeError::new(
                "file imports require an explicit .solx extension",
            ));
        }
        self.resolve_path(path, source_file.and_then(Path::parent))
            .map(ModuleId::File)
    }

    pub(crate) fn entry_path(&self, path: &Path) -> Result<PathBuf, RuntimeError> {
        self.resolve_path(path, None)
    }

    fn resolve_path(&self, path: &Path, directory: Option<&Path>) -> Result<PathBuf, RuntimeError> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            if path.has_root() || path.components().any(|c| matches!(c, Component::Prefix(_))) {
                return Err(RuntimeError::new(
                    "file paths must be relative or fully absolute",
                ));
            }
            let base = directory
                .or(self.base_directory.as_deref())
                .ok_or_else(|| {
                    RuntimeError::new(
                        "cannot resolve relative path: initial working directory is unavailable",
                    )
                })?;
            base.join(path)
        };
        fs::canonicalize(&absolute).map_err(|error| {
            RuntimeError::new(format!("cannot resolve '{}': {error}", absolute.display()))
        })
    }

    pub(crate) fn read(path: &Path) -> Result<String, RuntimeError> {
        fs::read_to_string(path).map_err(|error| {
            RuntimeError::new(format!("cannot read '{}': {error}", path.display()))
        })
    }

    fn check_cycle(&self, id: &ModuleId) -> Result<(), RuntimeError> {
        if let Some(start) = self.loading.iter().position(|loading| loading == id) {
            let chain = self.loading[start..]
                .iter()
                .chain(std::iter::once(id))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(RuntimeError::new(format!("circular import: {chain}")));
        }
        Ok(())
    }

    pub(crate) fn cached(&self, id: &ModuleId) -> Result<Option<Value>, RuntimeError> {
        self.check_cycle(id)?;
        Ok(self.cache.get(id).cloned())
    }

    pub(crate) fn begin(&mut self, id: &ModuleId) -> Result<(), RuntimeError> {
        self.check_cycle(id)?;
        self.loading.push(id.clone());
        Ok(())
    }

    pub(crate) fn end(&mut self, id: &ModuleId) {
        let last = self.loading.pop();
        debug_assert_eq!(last.as_ref(), Some(id));
    }

    pub(crate) fn cache(&mut self, id: ModuleId, value: Value) {
        self.cache.insert(id, value);
    }
}
