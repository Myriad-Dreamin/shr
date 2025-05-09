//! # shr
//!
//! shr hunts and reports disk space.

pub mod utils;

pub use event::*;
pub use path::*;
pub use scan::*;

mod event;
mod path;
mod scan;

/// A shortcut to run the [`Shr`].
pub async fn shr(dir: std::path::PathBuf) -> ShrRx {
    Shr::new(dir).run().await
}
