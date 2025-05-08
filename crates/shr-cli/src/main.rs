//! # shr
//!
//! shr hunts and reports disk space.

mod args;

use core::fmt;
use std::io::{self, Write};

use anyhow::Context;
use args::Format;
use clap::Parser;
use shr::{EventDisplay, ImmutPath};

use crate::args::Args;

// #[tokio::main]
#[compio::main]
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
                Some(EventDisplay::FileFinish { path, size }) => {
                    report_entry(&mut stdout, path, size, 0)?;
                }
                Some(EventDisplay::Entry { .. }) => {}
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
    let size = human_readable_number(size, "si");
    let path = path.as_ref().display();
    if num_files > 0 {
        writeln!(w, "{path} {size}, {num_files} file(s)")?;
    } else {
        writeln!(w, "{path} {size}")?;
    }
    Ok(())
}

// Borrowed from: https://github.com/bootandy/dust

static UNITS: [char; 5] = ['P', 'T', 'G', 'M', 'K'];

// If we are working with SI units or not
fn get_type_of_thousand(output_str: &str) -> u64 {
    if output_str.is_empty() {
        1024
    } else if output_str == "si" {
        1000
    } else if output_str.contains('i') || output_str.len() == 1 {
        1024
    } else {
        1000
    }
}

fn get_number_format(output_str: &str) -> Option<(u64, char)> {
    if output_str.starts_with('b') {
        return Some((1, 'B'));
    }
    for (i, u) in UNITS.iter().enumerate() {
        if output_str.starts_with((*u).to_ascii_lowercase()) {
            let marker = get_type_of_thousand(output_str).pow((UNITS.len() - i) as u32);
            return Some((marker, *u));
        }
    }
    None
}

struct Hr<'a>(u64, &'a str);

impl<'a> fmt::Display for Hr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = self.0;
        match get_number_format(self.1) {
            Some((x, u)) => {
                write!(f, "{}{}", (size / x), u)
            }
            None => {
                for (i, u) in UNITS.iter().enumerate() {
                    let marker = get_type_of_thousand(self.1).pow((UNITS.len() - i) as u32);
                    if size >= marker {
                        if size / marker < 10 {
                            return write!(f, "{:.1}{}", (size as f32 / marker as f32), u);
                        } else {
                            return write!(f, "{}{}", (size / marker), u);
                        }
                    }
                }
                write!(f, "{size}B")
            }
        }
    }
}

fn human_readable_number<'a>(size: u64, output_str: &'a str) -> Hr<'a> {
    Hr(size, output_str)
}
