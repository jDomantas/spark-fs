#![no_std]
#![feature(nll)]

#![allow(unused)]

pub mod io;
mod fs;
mod path;

pub use path::Path;
pub use fs::{Fd, FileSystem};
