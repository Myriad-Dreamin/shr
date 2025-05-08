use core::fmt;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "shr", version, about)]
pub struct Args {
    /// The directory to scan.
    #[clap()]
    dir: PathBuf,

    /// The output format.
    #[clap(long, default_value_t = Format::Du)]
    format: Format,
}

impl Args {
    /// Builds the `shr` instance.
    pub async fn build(self) -> (shr::ShrRx, Format) {
        let rx = shr::shr(self.dir).await;
        (rx, self.format)
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Format {
    Json,
    Du,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Json => write!(f, "json"),
            Format::Du => write!(f, "du"),
        }
    }
}
