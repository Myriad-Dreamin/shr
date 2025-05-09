use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use indexmap::IndexSet;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A path id that is used to identify a path.
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

/// A immutable path reference which can be serialized.
#[derive(Debug)]
pub struct ImmutPath(pub Arc<Path>);

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

/// A path interner that stores the paths and provides path ids.
#[derive(Debug)]
pub(crate) struct PathInterner {
    /// The mapping from paths to paths id.
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
    pub fn intern(&self, path: &Arc<Path>) -> PathId {
        let mut paths = self.paths.lock().unwrap();
        PathId(NonZeroUsize::new(paths.insert_full(path.clone()).0).unwrap())
    }

    /// Gets the path by id.
    pub fn get(&self, id: PathId) -> Option<Arc<Path>> {
        let paths = self.paths.lock().unwrap();
        paths.get_index(id.0.get()).cloned()
    }
}
