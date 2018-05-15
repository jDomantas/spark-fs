#![no_std]
#![feature(nll)]

#[cfg(test)]
#[macro_use]
extern crate std;

mod fs;
pub mod io;
mod path;

pub use fs::{format_storage, FileSystem};
pub use path::Path;
