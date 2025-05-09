//! rayon is used to avoid "Too many open files" error.

use rayon::iter::{ParallelBridge, ParallelIterator};
pub(crate) use tokio::sync::mpsc;

use crate::PathInterner;

use super::*;
/// The receiver for the events.
#[derive(Debug)]
pub struct ShrRx {
    pub(crate) path_interner: Arc<PathInterner>,
    pub(crate) rx: mpsc::UnboundedReceiver<Event>,
}

impl ShrRx {
    /// Receives an event.
    pub async fn recv(&mut self) -> Option<EventRef> {
        self.rx.recv().await.map(|data| EventRef { data, rx: self })
    }

    /// Gets the path for the given `PathId`.
    pub fn get_path(&self, id: PathId) -> Option<Arc<Path>> {
        self.path_interner.as_ref().get(id)
    }
}

pub(crate) struct Shared<'a> {
    /// The path interner.
    pub path_mgr: &'a PathInterner,
    /// The sender for the events.
    pub tx: mpsc::UnboundedSender<Event>,
    /// Whether to follow links.
    pub follow_links: bool,
}

/// The main struct.
pub(crate) struct ShrTask {
    /// The parent path id.
    pub parent: Option<PathId>,
    /// The path id to scan.
    pub path_id: PathId,
    /// The path to scan.
    pub path: Arc<Path>,
    /// The path to scan.
    pub remain_report_depth: usize,
}

impl ShrTask {
    /// Executes the task.
    pub fn exec(mut self, shared: &Shared) -> Option<(usize, u64)> {
        loop {
            let mt = std::fs::metadata(&self.path).report()?;
            if mt.is_file() {
                format_args!("scanning file: {:?}", self.path);
                let file_size = mt.len();

                if self.remain_report_depth > 0 {
                    let event = Event::FileFinish {
                        path: self.path_id,
                        parent: self.parent,
                        size: file_size,
                    };
                    let _ = shared.tx.send(event);
                }
                return Some((1, file_size));
            } else if mt.is_dir() {
                format_args!("scanning dir: {:?}", self.path);
                if self.remain_report_depth > 0 {
                    let event = Event::Dir {
                        path: self.path_id,
                        parent: self.parent,
                    };
                    let _ = shared.tx.send(event);
                }
                return self.scan_dir(shared);
            } else if shared.follow_links && mt.is_symlink() {
                format_args!("scanning link: {:?}", self.path);
                // Follow the link
                self.path = std::fs::read_link(&self.path).report()?.into();
            } else {
                format_args!("skip: {:?}", self.path);
                return Some((1, 0));
            }
        }
    }

    fn scan_dir(self, shared: &Shared) -> Option<(usize, u64)> {
        let tx = shared.tx.clone();
        let path_id = self.path_id;
        let remain_report_depth = self.remain_report_depth;

        let next_remain_report_depth = remain_report_depth.saturating_sub(1);
        let (num_files, size) = std::fs::read_dir(self.path.clone())
            .report()?
            .par_bridge()
            .fold(
                || (0, 0),
                |(num_files, size), entry| {
                    let Ok(entry) = entry else {
                        return (num_files, size);
                    };

                    let path = entry.path().into();
                    let task = Self {
                        remain_report_depth: next_remain_report_depth,
                        path_id: shared.path_mgr.intern(&path),
                        parent: Some(self.path_id),
                        path,
                    };

                    let (sub_num_files, sub_size) = task.exec(shared).unwrap_or((0, 0));
                    (num_files + sub_num_files, sub_size + size)
                },
            )
            .reduce(
                || (0, 0),
                |(num_files_a, size_a), (num_files_b, size_b)| {
                    (num_files_a + num_files_b, size_a + size_b)
                },
            );

        if remain_report_depth > 0 {
            let event = Event::DirFinish {
                path: path_id,
                size,
                num_files,
            };
            let _ = tx.send(event);
        }

        Some((num_files, size))
    }
}

trait Report {
    type Target;

    fn report(self) -> Option<Self::Target>
    where
        Self: Sized;
}

impl<T> Report for std::io::Result<T> {
    type Target = T;

    fn report(self) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("failed io: {e}");
                None
            }
        }
    }
}
