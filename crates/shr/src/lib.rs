//! # shr
//!
//! shr hunts and reports disk space.

pub mod utils;

pub use scan::*;

mod scan;

use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The scan routine.
pub async fn shr(dir: PathBuf) -> ShrRx {
    Shr::new(dir).run().await
}

/// The path id for shr
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PathId(NonZeroUsize);

impl PathId {
    /// Into a raw and unsafe `PathId` reference.
    pub fn into_raw(self) -> NonZeroUsize {
        self.0
    }

    /// Creates a new `PathId` instance.
    pub fn from_raw(id: NonZeroUsize) -> Self {
        Self(id)
    }
}

/// The event yield by `shr`.
#[derive(Debug)]
pub struct EventRef<'a> {
    rx: &'a ShrRx,
    data: Event,
}

impl From<EventRef<'_>> for Event {
    fn from(event: EventRef<'_>) -> Self {
        event.data
    }
}

impl EventRef<'_> {
    /// Converts to raw event.
    pub fn to_raw(self) -> Event {
        self.data
    }
}

impl EventRef<'_> {
    /// Creates a new `EventRef` instance.
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

/// The event yield by `shr`.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "camelCase"))]
pub enum Event {
    /// A file entry is found.
    Dir {
        /// The path to the entry.
        path: PathId,
        /// The parent directory.
        parent: Option<PathId>,
    },
    /// A directory is found.
    FileFinish {
        /// The path to the entry.
        path: PathId,
        /// The parent directory.
        parent: Option<PathId>,
        /// The size of the file in bytes, recursively.
        size: u64,
    },
    /// A directory is finished.
    DirFinish {
        /// The path to the entry.
        path: PathId,
        /// The size of the directory in bytes, recursively.
        size: u64,
        /// The number of files in the directory.
        num_files: usize,
    },
}

/// The short display type event.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "camelCase"))]
pub enum EventDisplay {
    /// A file is found.
    Dir {
        /// The path to the entry.
        path: Option<ImmutPath>,
        /// The parent directory.
        parent: Option<ImmutPath>,
    },
    /// A directory is found.
    FileFinish {
        /// The path to the entry.
        path: Option<ImmutPath>,
        /// The parent directory.
        parent: Option<ImmutPath>,
        /// The size of the file in bytes, recursively.
        size: u64,
    },
    /// A directory is finished.
    DirFinish {
        /// The path to the entry.
        path: Option<ImmutPath>,
        /// The size of the directory in bytes, recursively.
        size: u64,
        /// The number of files in the directory.
        num_files: usize,
    },
}

/// The immut path reference.
#[derive(Debug)]
pub struct ImmutPath(Arc<Path>);

impl AsRef<Arc<Path>> for ImmutPath {
    fn as_ref(&self) -> &Arc<Path> {
        &self.0
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ImmutPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_ref().serialize(serializer)
    }
}
