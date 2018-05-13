#![no_std]
#![feature(nll)]

#[cfg(test)]
extern crate std;

mod fs;
pub mod io;
mod path;

pub use fs::{format_storage, Fd, FileSystem};
pub use path::Path;
