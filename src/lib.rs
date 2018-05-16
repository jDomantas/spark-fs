#![no_std]
#![feature(nll)]

#[cfg(test)]
#[macro_use]
extern crate std;

pub mod fs;
pub mod io;
mod path;

pub use fs::FileSystem;
pub use path::Path;
