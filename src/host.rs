//! Explicit access grants for native filesystem and clock operations.
//!
//! These grants do not restrict source loading or file imports. They are not
//! a sandbox for the whole interpreter. Open handles are owned by the host
//! configuration and closed when it is replaced or its interpreter is dropped.

use cap_std::fs::Dir;
use std::{io, path::Path, time::Instant};

use crate::runtime::RuntimeError;

/// No filesystem or clock access is granted by default.
#[derive(Default)]
pub struct HostCapabilities {
    filesystem: Option<Filesystem>,
    clock: Option<Instant>,
}

struct Filesystem {
    directory: Dir,
    read: bool,
    write: bool,
}

impl HostCapabilities {
    /// Grant access relative to an existing directory, opened once here.
    /// Read and write are independent. The caller chooses a trusted root;
    /// paths supplied later by Solx are resolved using this directory handle.
    pub fn with_filesystem(
        mut self,
        root: impl AsRef<Path>,
        read: bool,
        write: bool,
    ) -> io::Result<Self> {
        self.filesystem = Some(Filesystem {
            directory: Dir::open_ambient_dir(root, cap_std::ambient_authority())?,
            read,
            write,
        });
        Ok(self)
    }

    /// Grant wall-clock access and start this configuration's monotonic clock.
    pub fn with_time(mut self) -> Self {
        self.clock = Some(Instant::now());
        self
    }

    pub(crate) fn directory(&self, write: bool, operation: &str) -> Result<&Dir, RuntimeError> {
        self.filesystem
            .as_ref()
            .filter(|fs| if write { fs.write } else { fs.read })
            .map(|fs| &fs.directory)
            .ok_or_else(|| {
                let permission = if write { "write" } else { "read" };
                RuntimeError::new(format!(
                    "{operation}() requires filesystem {permission} permission (--allow-{permission})"
                ))
            })
    }

    pub(crate) fn clock(&self, operation: &str) -> Result<Instant, RuntimeError> {
        self.clock.ok_or_else(|| {
            RuntimeError::new(format!(
                "{operation}() requires clock permission (--allow-time)"
            ))
        })
    }
}
