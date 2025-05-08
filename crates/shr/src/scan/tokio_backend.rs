pub(crate) use tokio::sync::mpsc;

use super::*;
/// The receiver for the events.
#[derive(Debug)]
pub struct ShrRx {
    path_mgr: Arc<PathInterner>,
    rx: mpsc::UnboundedReceiver<Event>,
}

impl ShrRx {
    /// Receives an event.
    pub async fn recv(&mut self) -> Option<EventRef> {
        self.rx.recv().await.map(|data| EventRef { data, rx: self })
    }

    /// Gets the path for the given `PathId`.
    pub fn get_path(&self, id: PathId) -> Option<Arc<Path>> {
        self.path_mgr.as_ref().get(id)
    }
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
    /// The file type.
    pub file_type: std::fs::FileType,
    /// The path interner.
    pub path_mgr: Arc<PathInterner>,
    /// The sender for the events.
    pub tx: mpsc::UnboundedSender<Event>,
    /// Whether to follow links.
    pub follow_links: bool,
}

impl ShrTask {
    /// Executes the task.
    pub async fn exec(mut self) -> Option<(usize, u64)> {
        loop {
            if self.file_type.is_file() {
                self.send_entry();
                // todo: this is sync
                let file_size = self.path.metadata().ok()?.len();

                if self.remain_report_depth > 0 {
                    let event = Event::FileFinish {
                        path: self.path_id,
                        size: file_size,
                    };
                    self.send(event);
                }
                return Some((1, file_size));
            } else if self.file_type.is_dir() {
                self.send_entry();
                return tokio::spawn(self.scan_dir()).await.ok()?;
            } else if self.follow_links && self.file_type.is_symlink() {
                // Follow the link
                let path: Arc<Path> = self.path.read_link().ok()?.into();

                self.path = path.clone();

                let metadata = self.path.metadata().ok()?;
                self.file_type = metadata.file_type();
            }
        }
    }

    fn scan_dir(self) -> Pin<Box<dyn Future<Output = Option<(usize, u64)>> + Send>> {
        Box::pin(async move {
            let tx = self.tx.clone();
            let path_id = self.path_id;
            let remain_report_depth = self.remain_report_depth;
            let iter = tokio::task::spawn_blocking(move || {
                std::io::Result::Ok(
                    std::fs::read_dir(&self.path)?
                        .flat_map(|t| self.dir_task(t.ok()?).map(|t| tokio::spawn(t.exec())))
                        .collect::<Box<_>>(),
                )
            })
            .await
            .ok()?
            .ok()?;
            let mut num_files = iter.len();
            let mut size = 0;
            for task in iter {
                let Some((num, s)) = task.await.ok().flatten() else {
                    continue;
                };
                num_files += num;
                size += s;
            }

            if remain_report_depth > 0 {
                let event = Event::DirFinish {
                    path: path_id,
                    size,
                    num_files,
                };
                let _ = tx.send(event);
            }

            Some((num_files, size))
        })
    }

    fn dir_task(&self, t: std::fs::DirEntry) -> Option<Self> {
        let path = t.path().into();
        Some(Self {
            remain_report_depth: self.remain_report_depth.saturating_sub(1),
            file_type: t.file_type().ok()?,
            path_id: self.path_mgr.intern(&path),
            parent: Some(self.path_id),
            path,
            path_mgr: self.path_mgr.clone(),
            tx: self.tx.clone(),
            follow_links: self.follow_links,
        })
    }

    fn send(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    fn send_entry(&self) {
        if self.remain_report_depth == 0 {
            return;
        }
        let event = Event::Entry {
            path: self.path_id,
            parent: self.parent,
            is_dir: self.file_type.is_dir(),
        };
        self.send(event);
    }
}
