//! Borrowed from: https://github.com/bootandy/dust

use core::fmt;

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

/// A struct to represent a human-readable number.
pub struct Hr<'a>(u64, &'a str);

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

/// Converts a number to a human-readable format.
/// output_str: `si` for SI units, `bi` for binary units, or `b` for bytes.
pub fn human_readable_number<'a>(size: u64, output_str: &'a str) -> Hr<'a> {
    Hr(size, output_str)
}
