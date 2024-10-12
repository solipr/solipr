//! Defines a [Change] struct used to represent a change to a repository.

use uuid::Uuid;

use crate::registry::ContentHash;
use crate::stack::StackVec;

/// The hash of a change stored in the registry.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangeHash(ContentHash);

/// The identifier of a file.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(Uuid);

/// The identifier of a line in a file.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineId(Uuid);

impl LineId {
    /// The identifier of the first line in a file.
    ///
    /// This is line is not a real line, it is used to indicate the beginning of
    /// the file.
    #[expect(dead_code, reason = "TODO: Remove this when we don't need it anymore")]
    const FIRST: Self = Self(Uuid::nil());

    /// The identifier of the last line in a file.
    ///
    /// This is line is not a real line, it is used to indicate the end of the
    /// file.
    #[expect(dead_code, reason = "TODO: Remove this when we don't need it anymore")]
    const LAST: Self = Self(Uuid::max());
}

/// A change.
///
/// TODO: Add the changes to modify files.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Change {
    /// Add a line to a file.
    DefineLine {
        /// The file in which the line is added.
        file_id: FileId,

        /// The line that is added.
        line_id: LineId,

        /// The content of the line.
        content: ContentHash,
    },

    /// Remove a line from a file.
    ///
    /// This change does not really remove the line, it only marks it as
    /// removed. To be fully removed, the parent and child lines must be updated
    /// with the [`Change::LineChild`] and [`Change::LineParent`] changes
    /// respectively to make them stop being linked to the removed line.
    ///
    /// For more information, look at
    /// [the OVG documentation](https://github.com/solipr/solipr/blob/main/docs/ovg.md).
    RemoveLine {
        /// The file in which the line is removed.
        file_id: FileId,

        /// The line that is removed.
        line_id: LineId,
    },

    /// Update the parent of a line.
    ///
    /// For more information, look at
    /// [the OVG documentation](https://github.com/solipr/solipr/blob/main/docs/ovg.md).
    LineParent {
        /// The file in which the line parent is updated.
        file_id: FileId,

        /// The line whose parent is updated.
        line_id: LineId,

        /// The changes replaced by this change.
        ///
        /// If there is more than 3 changes to be replaced, you should make one
        /// change to replace the first 3 changes and then make another change
        /// to replace the first created change with the rest.
        replace: StackVec<ChangeHash, 3>,

        /// The new parent of the line.
        parent: LineId,
    },

    /// Update the child of a line.
    ///
    /// For more information, look at
    /// [the OVG documentation](https://github.com/solipr/solipr/blob/main/docs/ovg.md).
    LineChild {
        /// The file in which the line child is updated.
        file_id: FileId,

        /// The line whose child is updated.
        line_id: LineId,

        /// The changes replaced by this change.
        ///
        /// If there is more than 3 changes to be replaced, you should make one
        /// change to replace the first 3 changes and then make another change
        /// to replace the first created change with the rest.
        replace: StackVec<ChangeHash, 3>,

        /// The new child of the line.
        child: LineId,
    },
}
