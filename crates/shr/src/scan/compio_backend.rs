mod rt;

pub(crate) mod mpsc {
    pub use futures::channel::mpsc::UnboundedReceiver;
    pub use futures::channel::mpsc::UnboundedSender;

    pub use futures::channel::mpsc::unbounded as unbounded_channel;
}

use futures::SinkExt;
use rt::CompioThread;

use super::*;

/// The receiver for the events.
#[derive(Debug)]
pub struct ShrRx {
    pub(crate) path_mgr: Arc<PathInterner>,
    pub(crate) rx: mpsc::UnboundedReceiver<Event>,
}

impl ShrRx {
    /// Receives an event.
    pub async fn recv(&mut self) -> Option<EventRef> {
        // todo: ok
        self.rx
            .try_next()
            .ok()
            .flatten()
            .map(|data| EventRef { data, rx: self })
    }

    /// Gets the path for the given `PathId`.
    pub fn get_path(&self, id: PathId) -> Option<Arc<Path>> {
        self.path_mgr.as_ref().get(id)
    }
}

impl Shr {
    pub(crate) fn rt() -> Arc<CompioThread> {
        Arc::new(rt::CompioThread::new(compio::runtime::RuntimeBuilder::new()))
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
    /// The path interner.
    pub path_mgr: Arc<PathInterner>,
    /// The sender for the events.
    pub tx: mpsc::UnboundedSender<Event>,
    /// Whether to follow links.
    pub follow_links: bool,
    /// The dispatcher sender.
    pub rt: Arc<CompioThread>,
    /// The file type.
    pub file_type: FileType,
}

impl ShrTask {
    /// Executes the task.
    pub async fn exec(mut self) -> Option<(usize, u64)> {
        loop {
            let file = compio::fs::File::open(&self.path).await.ok()?;

            if self.file_type.is_file() {
                self.send_entry();
                // todo: this is sync
                let file_size = file.metadata().await.ok()?.len();

                if self.remain_report_depth > 0 {
                    let event = Event::FileFinish {
                        path: self.path_id,
                        size: file_size,
                    };
                    let _ = self.tx.send(event);
                }
                return Some((1, file_size));
            } else if self.file_type.is_dir() {
                self.send_entry();
                return self
                    .rt
                    .clone()
                    .spawn_read_dir(self)
                    .await
                    .ok()?
                    .wait()
                    .await
                    .ok()
                    .flatten();
            } else if self.follow_links && self.file_type.is_symlink() {
                // Follow the link
                let path: Arc<Path> = self.path.read_link().ok()?.into();

                self.path = path.clone();

                let metadata = file.metadata().await.ok()?;
                self.file_type = metadata.file_type().into();
            }
        }
    }

    async fn scan_dir(self) -> Option<(usize, u64)> {
        let mut tx = self.tx.clone();
        let path_id = self.path_id;
        let remain_report_depth = self.remain_report_depth;
        let path = self.path.clone();
        let iter = std::thread::spawn(|| std::fs::read_dir(path).unwrap())
            .join()
            .unwrap()
            .flat_map(|t| self.dir_task(t.ok()?).map(|t| self.rt.spawn(t)))
            .collect::<Box<_>>();
        let mut num_files = iter.len();
        let mut size = 0;
        for task in iter {
            let Some(task) = task.await.ok() else {
                continue;
            };
            let Some((num, s)) = task.wait().await.ok()? else {
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
    }

    fn dir_task(&self, t: std::fs::DirEntry) -> Option<Self> {
        let path = t.path().into();
        Some(Self {
            remain_report_depth: self.remain_report_depth.saturating_sub(1),
            file_type: t.file_type().ok()?.into(),
            path_id: self.path_mgr.intern(&path),
            parent: Some(self.path_id),
            path,
            path_mgr: self.path_mgr.clone(),
            tx: self.tx.clone(),
            follow_links: self.follow_links,
            rt: self.rt.clone(),
        })
    }

    fn send_entry(&mut self) {
        if self.remain_report_depth == 0 {
            return;
        }
        let event = Event::Entry {
            path: self.path_id,
            parent: self.parent,
            is_dir: self.file_type.is_dir(),
        };
        let _ = self.tx.send(event);
    }
}

pub(crate) enum FileType {
    Std(std::fs::FileType),
    Compio(compio::fs::FileType),
}

impl From<std::fs::FileType> for FileType {
    fn from(file_type: std::fs::FileType) -> Self {
        Self::Std(file_type)
    }
}

impl From<compio::fs::FileType> for FileType {
    fn from(file_type: compio::fs::FileType) -> Self {
        Self::Compio(file_type)
    }
}

impl FileType {
    fn is_file(&self) -> bool {
        match self {
            Self::Std(file_type) => file_type.is_file(),
            Self::Compio(file_type) => file_type.is_file(),
        }
    }

    fn is_dir(&self) -> bool {
        match self {
            Self::Std(file_type) => file_type.is_dir(),
            Self::Compio(file_type) => file_type.is_dir(),
        }
    }

    fn is_symlink(&self) -> bool {
        match self {
            Self::Std(file_type) => file_type.is_symlink(),
            Self::Compio(file_type) => file_type.is_symlink(),
        }
    }
}
