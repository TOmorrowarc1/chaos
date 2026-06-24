#![cfg_attr(not(test), no_std)]
#![feature(nll)]
#![deny(non_snake_case)]

// import macros from log
use log::*;
extern crate alloc;

mod addr;
pub mod memory_set;
pub mod paging;

pub use crate::addr::*;

pub enum VMError {
    InvalidPtr,
}

pub type VMResult<T> = Result<T, VMError>;
