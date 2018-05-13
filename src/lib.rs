#![no_std]
#![feature(nll)]
#![allow(unused)]

mod fs;
pub mod io;
mod path;

pub use fs::{Fd, FileSystem};
pub use path::Path;
