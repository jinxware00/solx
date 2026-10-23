//! Source identities, path resolution, loading state, and per-interpreter cache.
//! Source execution stays in the interpreter; no evaluator logic lives here.
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::limits::LimitKind;
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
    pub(crate) fn release_roots(&mut self) {
        self.cache.clear();
    }

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

    pub(crate) fn read(path: &Path, max_source_bytes: usize) -> Result<String, RuntimeError> {
        let read_error = |error| {
            RuntimeError::new(format!("cannot read '{}': {error}", path.display()))
                .at(Some(path), None)
        };
        let file = fs::File::open(path).map_err(read_error)?;
        let bytes = read_source_bytes(file, max_source_bytes).map_err(read_error)?;
        if bytes.len() > max_source_bytes {
            return Err(
                RuntimeError::limit(LimitKind::SourceBytes, max_source_bytes).at(Some(path), None),
            );
        }
        String::from_utf8(bytes).map_err(|_| {
            RuntimeError::new(format!(
                "cannot read '{}': source must be valid UTF-8",
                path.display()
            ))
            .at(Some(path), None)
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

fn read_source_bytes(reader: impl Read, maximum: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    // The extra byte distinguishes an exact fit from oversized input. Saturation
    // also permits host-selected usize::MAX without arithmetic overflow.
    let limit = u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1);
    reader.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_reader_stops_at_one_byte_past_the_budget_even_without_eof() {
        for maximum in [0, 1, 17, 1_048_576] {
            let bytes = read_source_bytes(io::repeat(b'x'), maximum).unwrap();
            assert_eq!(bytes.len(), maximum + 1);
        }
    }

    #[test]
    fn source_reader_preserves_exact_boundary_bytes_and_large_host_budgets() {
        let source = "--\u{e9}".as_bytes();
        assert_eq!(read_source_bytes(source, source.len()).unwrap(), source);
        assert_eq!(read_source_bytes(source, usize::MAX).unwrap(), source);
    }
}
