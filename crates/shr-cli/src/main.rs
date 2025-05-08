//! # shr
//!
//! shr hunts and reports disk space.

mod args;

use std::io::{self, Write};

use anyhow::Context;
use args::Format;
use clap::Parser;
use shr::{EventDisplay, ImmutPath};

use crate::args::Args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (mut rx, f) = Args::parse().build().await;
    let mut stdout = std::io::stdout().lock();
    match f {
        Format::Du => loop {
            let event = rx.recv().await.map(|event| event.display());
            match event {
                Some(EventDisplay::DirFinish {
                    path,
                    size,
                    num_files,
                }) => {
                    report_entry(&mut stdout, path, size, num_files)?;
                }
                Some(EventDisplay::FileFinish {
                    path,
                    size,
                    parent: _,
                }) => {
                    report_entry(&mut stdout, path, size, 0)?;
                }
                Some(EventDisplay::Dir { .. }) => {}
                None => break,
            }
        },
        Format::Json => loop {
            let event = rx.recv().await;
            match event {
                Some(event) => {
                    serde_json::to_writer(&mut stdout, &event)
                        .context("failed to serialize event")?;
                    stdout.write_all(b"\n").context("failed to write newline")?;
                }
                None => break,
            }
        },
    }

    Ok(())
}

fn report_entry(
    w: &mut impl Write,
    path: Option<ImmutPath>,
    size: u64,
    num_files: usize,
) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let size = shr::utils::human_readable_number(size, "si");
    let path = path.as_ref().display();
    if num_files > 0 {
        writeln!(w, "{path} {size}, {num_files} file(s)")?;
    } else {
        writeln!(w, "{path} {size}")?;
    }
    Ok(())
}
