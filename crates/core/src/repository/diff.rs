//! Implement a trait extention that add functions to find change to update a
//! repository.

use std::collections::HashSet;

use solipr_stack::StackVec;

use super::Repository;
use crate::change::{Change, ChangeContent};

/// A trait extention that add functions to retrieve information from the heads
/// of the SVG in the repository.
pub trait DiffExt<'manager>: Repository<'manager> {
    
}

impl<'manager, T: Repository<'manager>> DiffExt<'manager> for T {}
