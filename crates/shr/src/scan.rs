use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use indexmap::IndexSet;

use crate::{Event, EventRef, PathId};

#[cfg(feature = "tokio")]
mod tokio_backend;
#[cfg(feature = "tokio")]
pub use tokio_backend::*;

#[cfg(feature = "compio")]
mod compio_backend;
#[cfg(feature = "compio")]
pub use compio_backend::*;

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
        }
    }

    /// Runs the scan routine.
    pub async fn run(self) -> ShrRx {
        let rx = self.rx;
        let path_mgr = self.path_mgr;

        if let Ok(meta) = self.path.metadata() {
            let path = self.path.into();
            let task = ShrTask {
                rt: Self::rt(),
                path_id: path_mgr.intern(&path),
                parent: None,
                path,
                file_type: meta.file_type().into(),
                path_mgr: path_mgr.clone(),
                tx: self.tx,
                remain_report_depth: 1,
                follow_links: true,
            };
            task.exec().await;
        }

        ShrRx { path_mgr, rx }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PathInterner {
    /// The paths.
    paths: Mutex<IndexSet<Arc<Path>>>,
}

impl PathInterner {
    /// Interns a path.
    fn intern(&self, path: &Arc<Path>) -> PathId {
        let mut paths = self.paths.lock().unwrap();
        PathId(paths.insert_full(path.clone()).0)
    }

    fn get(&self, id: PathId) -> Option<Arc<Path>> {
        let paths = self.paths.lock().unwrap();
        paths.get_index(id.0).cloned()
    }
}
