#![no_std]
#![feature(nll)]

#[cfg(test)]
extern crate std;

mod fs;
pub mod io;
mod path;

pub use fs::{Fd, FileSystem, format_storage};
pub use path::Path;
