//! This crate implements the scan function.
//!
//! ## Backends
//!
//! It has a tokio backend. There was a compio backend, but I deleted it for
//! buggy code. We may add other backend in future.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{Event, EventRef, PathId, PathInterner};

#[cfg(feature = "tokio")]
mod tokio_backend;
#[cfg(feature = "tokio")]
pub use tokio_backend::*;

/// The main struct to scan the directory recursively.
pub struct Shr {
    /// The path to scan.
    path: PathBuf,
    /// The path interner.
    path_interner: Arc<PathInterner>,
    /// The maximum depth to report.
    max_depth: usize,
}

impl Shr {
    /// Creates a `Shr` that scans files in the `path`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            path_interner: Arc::new(PathInterner::default()),
            max_depth: usize::MAX,
        }
    }

    /// Sets the maximum depth to *report*. That is, all the files under the
    /// directory are still scanned but only the files whose path is less than
    /// `max_depth` is printed.
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth.saturating_add(1);
        self
    }

    /// Runs the scan routine.
    pub async fn run(self) -> ShrRx {
        let (tx, rx) = mpsc::unbounded_channel();
        let path_interner = self.path_interner;

        let path = self.path.into();
        let task = ShrTask {
            path_id: path_interner.intern(&path),
            parent: None,
            path,
            remain_report_depth: self.max_depth,
        };
        let path_mgr2 = path_interner.clone();
        tokio::spawn(tokio::task::spawn_blocking(move || {
            let shared = Shared {
                path_mgr: &path_mgr2,
                tx,
                follow_links: true,
            };
            task.exec(&shared)
        }));

        ShrRx { path_interner, rx }
    }
}
