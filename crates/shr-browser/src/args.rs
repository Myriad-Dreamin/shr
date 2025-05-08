use std::path::PathBuf;

use clap::Parser;
use shr::Shr;

#[derive(Debug, Parser)]
pub struct Args {
    /// The directory to scan.
    #[clap()]
    dir: PathBuf,
}

impl Args {
    /// Builds the `shr` instance.
    pub async fn build(self) -> shr::ShrRx {
        Shr::new(self.dir).with_max_depth(usize::MAX).run().await
    }
}
