use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use indexmap::IndexSet;

use crate::{Event, EventRef, PathId};

#[cfg(feature = "tokio")]
mod tokio_backend;
#[cfg(feature = "tokio")]
pub use tokio_backend::*;

/// The main struct.
pub struct Shr {
    /// The path to scan.
    path: PathBuf,
    /// The path interner.
    path_mgr: Arc<PathInterner>,
    /// The sender for the events.
    tx: mpsc::UnboundedSender<Event>,
    /// The receiver for the events.
    rx: mpsc::UnboundedReceiver<Event>,
    max_depth: usize,
}

impl Shr {
    /// Creates a new `Shr` instance.
    pub fn new(path: PathBuf) -> Self {
        let path_mgr = Arc::new(PathInterner::default());
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            path,
            path_mgr,
            tx,
            rx,
            max_depth: usize::MAX,
        }
    }

    /// Sets the maximum depth for the scan.
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth.saturating_add(1);
        self
    }

    /// Runs the scan routine.
    pub async fn run(self) -> ShrRx {
        let rx = self.rx;
        let tx = self.tx;
        let path_mgr = self.path_mgr;

        let path = self.path.into();
        let task = ShrTask {
            path_id: path_mgr.intern(&path),
            parent: None,
            path,
            remain_report_depth: self.max_depth,
        };
        let path_mgr2 = path_mgr.clone();
        tokio::spawn(tokio::task::spawn_blocking(move || {
            let shared = Shared {
                path_mgr: &path_mgr2,
                tx,
                follow_links: true,
            };
            task.exec(&shared)
        }));

        ShrRx { path_mgr, rx }
    }
}

#[derive(Debug)]
pub(crate) struct PathInterner {
    /// The paths.
    paths: Mutex<IndexSet<Arc<Path>>>,
}

impl Default for PathInterner {
    fn default() -> Self {
        let mut paths = IndexSet::new();
        paths.insert(PathBuf::from("_never_touched_zero_").into());
        Self {
            paths: Mutex::new(paths),
        }
    }
}

impl PathInterner {
    /// Interns a path.
    fn intern(&self, path: &Arc<Path>) -> PathId {
        let mut paths = self.paths.lock().unwrap();
        PathId(NonZeroUsize::new(paths.insert_full(path.clone()).0).unwrap())
    }

    fn get(&self, id: PathId) -> Option<Arc<Path>> {
        let paths = self.paths.lock().unwrap();
        paths.get_index(id.0.get()).cloned()
    }
}
