#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{ImmutPath, PathId, ShrRx};

/// A event reference yield by `shr`.
#[derive(Debug)]
pub struct EventRef<'a> {
    /// The reference to the path data.
    pub(crate) rx: &'a ShrRx,
    /// The raw owned data.
    pub(crate) data: Event,
}

impl From<EventRef<'_>> for Event {
    fn from(event: EventRef<'_>) -> Self {
        event.data
    }
}

impl EventRef<'_> {
    /// Converts to a raw event.
    pub fn to_raw(self) -> Event {
        self.data
    }

    /// Collects path for display.
    pub fn display(&self) -> EventDisplay {
        match self.data {
            Event::Dir { path, parent } => EventDisplay::Dir {
                path: self.rx.get_path(path).map(ImmutPath),
                parent: parent.and_then(|parent| self.rx.get_path(parent).map(ImmutPath)),
            },
            Event::FileFinish { path, parent, size } => EventDisplay::FileFinish {
                path: self.rx.get_path(path).map(ImmutPath),
                parent: parent.and_then(|parent| self.rx.get_path(parent).map(ImmutPath)),
                size,
            },
            Event::DirFinish {
                path,
                size,
                num_files,
            } => EventDisplay::DirFinish {
                path: self.rx.get_path(path).map(ImmutPath),
                size,
                num_files,
            },
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for EventRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.display().serialize(serializer)
    }
}

/// A raw event yield by `shr`.
pub type Event = EventModel<PathId, Option<PathId>>;
/// The struct to display paths in event.
pub type EventDisplay = EventModel<Option<ImmutPath>, Option<ImmutPath>>;

/// A event yield by `shr`.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "camelCase"))]
pub enum EventModel<ThisP, ParentP> {
    /// A directory is found.
    Dir {
        /// The path to the entry.
        path: ThisP,
        /// The parent directory.
        parent: ParentP,
    },
    /// A file is finished.
    FileFinish {
        /// The path to the entry.
        path: ThisP,
        /// The parent directory.
        parent: ParentP,
        /// The size of the file in bytes, recursively.
        size: u64,
    },
    /// A directory is finished.
    DirFinish {
        /// The path to the entry.
        path: ThisP,
        /// The size of the directory in bytes, recursively.
        size: u64,
        /// The number of files in the directory.
        num_files: usize,
    },
}
