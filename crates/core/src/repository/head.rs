//! Implement a trait extention that add functions to retrieve information from
//! the heads of the SVG in the repository.

use std::collections::HashSet;

use super::Repository;
use crate::change::{Change, ChangeContent, FileId, LineId, SingleId};
use crate::registry::ContentHash;

/// A trait extention that add functions to retrieve information from the heads
/// of the SVG in the repository.
pub trait HeadExt<'manager>: Repository<'manager> {
    /// Returns the existence of the given [`LineId`].
    ///
    /// If there is an existence conflict, `None` will be returned. If there
    /// is no conflict, `Some(true)` or `Some(false)` will be returned if the
    /// line exists or not respectively.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_existence(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<Option<bool>, Self::Error> {
        let heads = self.heads(SingleId::LineExistence(file_id, line_id))?;
        let mut current_value = None;
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineExistence { existence, .. },
                ..
            }) = self.change(head)?
            {
                if current_value.is_none() {
                    current_value = Some(existence);
                } else if current_value != Some(existence) {
                    return Ok(None);
                }
            }
        }
        Ok(Some(current_value.unwrap_or(false)))
    }

    /// Returns the content of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no content
    /// has been set, an empty set will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_content(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<HashSet<ContentHash>, Self::Error> {
        let heads = self.heads(SingleId::LineContent(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineContent { content, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(content);
            }
        }
        Ok(result)
    }

    /// Returns the parent of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no parent
    /// has been set, [`LineId::FIRST`] will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_parent(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<HashSet<LineId>, Self::Error> {
        if line_id == LineId::FIRST {
            return Ok(HashSet::new());
        }
        let heads = self.heads(SingleId::LineParent(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineParent { parent, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(parent);
            }
        }
        if result.is_empty() {
            result.insert(LineId::FIRST);
        }
        Ok(result)
    }

    /// Returns the child of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no child
    /// has been set, [`LineId::LAST`] will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_child(&self, file_id: FileId, line_id: LineId) -> Result<HashSet<LineId>, Self::Error> {
        if line_id == LineId::LAST {
            return Ok(HashSet::new());
        }
        let heads = self.heads(SingleId::LineChild(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineChild { child, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(child);
            }
        }
        if result.is_empty() {
            result.insert(LineId::LAST);
        }
        Ok(result)
    }
}

impl<'manager, T: Repository<'manager>> HeadExt<'manager> for T {}
