//! Define a collection of data structures without any allocations for Solipr.

#![feature(stmt_expr_attributes)]

pub mod vec;

pub use vec::StackVec;
