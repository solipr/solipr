//! Defines a [Change] struct used to represent a change to a repository.

use std::fmt::{self, Debug, Display};
use std::ops::Deref;
use std::str::FromStr;

use base64::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};
use solipr_stack::StackVec;
use uuid::Uuid;

use crate::registry::ContentHash;

/// The hash of a change stored in the registry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct ChangeHash([u8; 32]);

impl Debug for ChangeHash {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ChangeHash")
            .field(&format_args!("{}", BASE64_URL_SAFE_NO_PAD.encode(self.0)))
            .finish()
    }
}

impl Display for ChangeHash {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "change:{}", BASE64_URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl FromStr for ChangeHash {
    type Err = base64::DecodeSliceError;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("change:").unwrap_or(value);
        let mut buffer = [0; 32];
        BASE64_URL_SAFE_NO_PAD.decode_slice(value.as_bytes(), &mut buffer)?;
        Ok(Self(buffer))
    }
}

/// The identifier of a file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize,
)]
pub struct FileId(Uuid);

impl Display for FileId {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file:{}", self.0)
    }
}

impl FromStr for FileId {
    type Err = uuid::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("file:").unwrap_or(value);
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl Deref for FileId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The identifier of a line in a file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize,
)]
pub struct LineId(Uuid);

impl LineId {
    /// The identifier of the first line in a file.
    ///
    /// This is line is not a real line, it is used to indicate the beginning of
    /// the file.
    pub const FIRST: Self = Self(Uuid::nil());

    /// The identifier of the last line in a file.
    ///
    /// This is line is not a real line, it is used to indicate the end of the
    /// file.
    pub const LAST: Self = Self(Uuid::max());

    /// The identifier of an unknown line in a file.
    ///
    /// This is the identifier used for each line read from a file before using
    /// the diff algorithm to detect changes.
    pub const UNKNOWN: Self = Self(Uuid::from_bytes([1; 16]));
}

impl Display for LineId {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line:{}", self.0)
    }
}

impl FromStr for LineId {
    type Err = uuid::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("line:").unwrap_or(value);
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl Deref for LineId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The identifier of the SVG modified by a [Change].
///
/// For more information, look at
/// [the SVG documentation](https://github.com/solipr/solipr/blob/main/docs/svg.md).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize,
)]
pub enum SingleId {
    /// The [Change] updates the existence of a line.
    LineExistence(FileId, LineId),

    /// The [Change] updates the content of a line.
    LineContent(FileId, LineId),

    /// The [Change] updates the parent of a line.
    LineChild(FileId, LineId),

    /// The [Change] updates the child of a line.
    LineParent(FileId, LineId),
}

/// A change that can be applied to a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize)]
pub struct Change {
    /// The changes replaced by this change.
    ///
    /// If there is more than 3 changes to be replaced, you should make one
    /// change to replace the first 3 changes and then make another change
    /// to replace the first created change with the rest.
    pub replace: StackVec<ChangeHash, 3>,

    /// The content of the change.
    pub content: ChangeContent,
}

impl Change {
    /// The SVG modified by this change.
    ///
    /// For more information, look at
    /// [the SVG documentation](https://github.com/solipr/solipr/blob/main/docs/svg.md).
    #[must_use]
    pub const fn single_id(&self) -> SingleId {
        self.content.single_id()
    }

    /// Returns the hash of this change.
    #[must_use]
    pub fn calculate_hash(&self) -> ChangeHash {
        let mut hasher = Sha256::new();
        #[expect(clippy::unused_result_ok, reason = "writing to hasher can't fail")]
        borsh::to_writer(&mut hasher, self).ok();
        ChangeHash(hasher.finalize().into())
    }
}

/// The content of a [Change].
///
/// TODO: Add the changes to modify files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize)]
pub enum ChangeContent {
    /// Update the Existence of a line.
    ///
    /// This change alone cannot really remove the line, it only marks it as
    /// removed. To be fully removed, the parent and child lines must be updated
    /// with the [`ChangeContent::LineChild`] and [`ChangeContent::LineParent`]
    /// changes respectively to make them stop being linked to the removed
    /// line.
    ///
    /// For more information, look at
    /// [the OVG documentation](https://github.com/solipr/solipr/blob/main/docs/ovg.md).
    LineExistence {
        /// The file modified by this change.
        file_id: FileId,

        /// The line modified by this change.
        line_id: LineId,

        /// The new existence of the line.
        existence: bool,
    },

    /// Update the content of a line.
    LineContent {
        /// The file modified by this change.
        file_id: FileId,

        /// The line modified by this change.
        line_id: LineId,

        /// The new content of the line.
        content: ContentHash,
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

        /// The new child of the line.
        child: LineId,
    },
}

impl ChangeContent {
    /// Returns the SVG modified by this [`ChangeContent`].
    ///
    /// For more information, look at
    /// [the SVG documentation](https://github.com/solipr/solipr/blob/main/docs/svg.md).
    #[must_use]
    pub const fn single_id(&self) -> SingleId {
        match *self {
            Self::LineExistence {
                file_id, line_id, ..
            } => SingleId::LineExistence(file_id, line_id),
            Self::LineContent {
                file_id, line_id, ..
            } => SingleId::LineContent(file_id, line_id),
            Self::LineParent {
                file_id, line_id, ..
            } => SingleId::LineParent(file_id, line_id),
            Self::LineChild {
                file_id, line_id, ..
            } => SingleId::LineChild(file_id, line_id),
        }
    }
}
