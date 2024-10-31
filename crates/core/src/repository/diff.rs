//! Implement a trait extention that add functions to find change to update a
//! repository.

use std::collections::HashSet;

use solipr_stack::StackVec;

use super::Repository;
use crate::change::{Change, ChangeContent};

/// A trait extention that add functions to retrieve information from the heads
/// of the SVG in the repository.
pub trait DiffExt<'manager>: Repository<'manager> {
    /// Returns the changes needed to replace the current value of an SVG.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn svg_diff(&self, new_content: ChangeContent) -> Result<HashSet<Change>, Self::Error> {
        let mut result = HashSet::new();
        let mut heads = Vec::from_iter(self.heads(new_content.single_id())?);
        while !heads.is_empty() {
            let mut replaced_heads = StackVec::new();
            while !heads.is_empty() && !replaced_heads.is_full() {
                #[expect(clippy::unwrap_used, reason = "heads is not empty")]
                replaced_heads.push(heads.pop().unwrap());
            }
            let change = Change {
                replace: replaced_heads,
                content: new_content,
            };
            result.insert(change);
            if !heads.is_empty() {
                heads.insert(0, change.calculate_hash());
            }
        }
        Ok(result)
    }
}

impl<'manager, T: Repository<'manager>> DiffExt<'manager> for T {}
