//! Defines a [`RepositoryManager`] and [Repository] traits.
//!
//! These traits are used to open repositories, apply changes to them and
//! retrieve information from them.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt::{self, Display};

use borsh::{BorshDeserialize, BorshSerialize};
use uuid::Uuid;

use crate::change::{Change, ChangeContent, ChangeHash, FileId, LineId, SingleId};
use crate::registry::ContentHash;
use crate::stack::StackVec;

pub mod persistent;

/// The identifier of a repository.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize,
)]
pub struct RepositoryId(Uuid);

impl RepositoryId {
    /// Creates a new [`RepositoryId`] that is guaranteed to be unique.
    #[must_use]
    pub fn create_new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Display for RepositoryId {
    #[expect(clippy::min_ident_chars, reason = "The trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "repo:{}", self.0)
    }
}

/// A [Repository] manager, used to open repositories.
pub trait RepositoryManager {
    /// The error that can be returned when opening a repository.
    type Error: Error;

    /// The type of [Repository] returned when opening a repository.
    type Repository<'manager>: Repository<'manager>
    where
        Self: 'manager;

    /// Opens a repository with a read-only access.
    ///
    /// If the repository does not exist, an empty repository will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    fn open_read(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error>;

    /// Opens a repository with a read-write access.
    ///
    /// If the repository does not exist, it will be created.
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    fn open_write(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error>;
}

/// A repository.
pub trait Repository<'manager> {
    /// The error that can be returned when doing a repository operation.
    type Error: Error;

    /// Returns an [Iterator] over the [Change]s applied to the repository.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn changes(&self) -> impl Iterator<Item = Result<(ChangeHash, Change), Self::Error>>;

    /// Returns a [Change] with the given [`ChangeHash`].
    ///
    /// If the change does not exist, `None` will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn change(&self, change_hash: ChangeHash) -> Result<Option<Change>, Self::Error>;

    /// Returns the heads of the given [`SingleId`].
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    ///
    /// # Note
    ///
    /// The default implementation is very inefficient and should be overridden
    /// if possible.
    fn heads(&self, single_id: SingleId) -> Result<HashSet<ChangeHash>, Self::Error> {
        let single_changes = self
            .changes()
            .filter(|change| {
                change
                    .as_ref()
                    .map(|&(_, change)| change.single_id() == single_id)
                    .unwrap_or(true)
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let mut heads = HashSet::new();
        'outer: for &(change_hash, _) in &single_changes {
            for &(_, other) in &single_changes {
                if other.replace.contains(&change_hash) {
                    continue 'outer;
                }
            }
            heads.insert(change_hash);
        }
        Ok(heads)
    }

    /// Returns the the existing [`LineId`]s in a file of the [Repository].
    ///
    /// If the existence of a line was not defined, it is considered to not
    /// exist.
    ///
    /// If the existence of a line is in a conflict state, this function will
    /// return it.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    ///
    /// # Note
    ///
    /// The default implementation is very inefficient and should be overridden
    /// if possible.
    fn existing_lines(&self, file_id: FileId) -> Result<HashSet<LineId>, Self::Error> {
        // Get all the lines in the file that have an existence change
        let file_lines = self
            .changes()
            .filter_map(|change| {
                change
                    .map(|(_, change)| match change.content {
                        ChangeContent::LineExistence {
                            file_id: change_file_id,
                            line_id,
                            existence,
                            ..
                        } if change_file_id == file_id && existence => Some(line_id),
                        ChangeContent::LineExistence { .. }
                        | ChangeContent::LineContent { .. }
                        | ChangeContent::LineParent { .. }
                        | ChangeContent::LineChild { .. } => None,
                    })
                    .transpose()
            })
            .collect::<Result<HashSet<_>, _>>()?;

        // Filter out the ones that don't exist
        let mut result = HashSet::new();
        for line_id in file_lines {
            if let Some(true) | None = self.line_existence(file_id, line_id)? {
                result.insert(line_id);
            }
        }

        // Return the result
        Ok(result)
    }

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

    /// Returns a [`FileGraph`] corresponding to the content of a file in the
    /// repository.
    ///
    /// This graph represent a particular state of an OVG but the missing links
    /// are also stored. For example if in the repository, a line `A` has a
    /// child `B` but `B` don't have `A` as a parent, this link will be added to
    /// this graph.
    ///
    /// The missing links are useful because they make deletion conflicts
    /// visible.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn file_graph(&self, file_id: FileId) -> Result<FileGraph, Self::Error> {
        let mut current = BTreeSet::from_iter(self.existing_lines(file_id)?);
        current.extend([LineId::FIRST, LineId::LAST]);
        let mut graph: HashMap<LineId, FileLine> = HashMap::new();

        // Find all the lines in the graph
        while let Some(line_id) = current.pop_first() {
            let line = FileLine {
                parent: self.line_parent(file_id, line_id)?,
                child: self.line_child(file_id, line_id)?,
                content: self.line_content(file_id, line_id)?,
            };

            // Search for the parents and children in the graph
            for parent in &line.parent {
                if !graph.contains_key(parent) && !current.contains(parent) {
                    current.insert(*parent);
                }
            }
            for child in &line.child {
                if !graph.contains_key(child) && !current.contains(child) {
                    current.insert(*child);
                }
            }

            // Add the line to the graph
            graph.insert(line_id, line);
        }

        // Generate the missing links
        #[expect(
            clippy::needless_collect,
            reason = "the collect is needed to make the borrow checker happy, without it we can't \
                      mutate the graph in the loop"
        )]
        #[expect(
            clippy::indexing_slicing,
            reason = "if a line has a parent or a child, it must be in the graph at this point"
        )]
        for line_id in graph.keys().copied().collect::<Vec<_>>() {
            for parent in graph[&line_id].parent.clone() {
                if let Some(line) = graph.get_mut(&parent) {
                    line.child.insert(line_id);
                }
            }
            for child in graph[&line_id].child.clone() {
                if let Some(line) = graph.get_mut(&child) {
                    line.parent.insert(line_id);
                }
            }
        }

        // Return the graph
        Ok(FileGraph(graph))
    }

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

    /// Returns a list of [Change] that transform a file of the [Repository]
    /// into the given [`FileGraph`].
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn file_graph_diff(
        &self,
        file_id: FileId,
        graph: &FileGraph,
    ) -> Result<HashSet<Change>, Self::Error> {
        let current_graph = self.file_graph(file_id)?;
        let mut result = HashSet::new();

        // Delete all the lines that are in the repository but not in the graph
        for line_id in current_graph.0.keys().copied() {
            if !graph.0.contains_key(&line_id) {
                result.extend(self.svg_diff(ChangeContent::LineExistence {
                    file_id,
                    line_id,
                    existence: false,
                })?);
            }
        }

        // Add all the lines that are in the graph but not in the repository
        for line_id in graph.0.keys().copied() {
            if !current_graph.0.contains_key(&line_id) {
                result.extend(self.svg_diff(ChangeContent::LineExistence {
                    file_id,
                    line_id,
                    existence: true,
                })?);
            }
        }

        // Update the links and content for each line of the graph
        for (&line_id, line) in &graph.0 {
            // Update the parent
            let current_parent = current_graph.0.get(&line_id).map(|line| &line.parent);
            if current_parent != Some(&line.parent) {
                for parent in line.parent.iter().copied() {
                    result.extend(self.svg_diff(ChangeContent::LineParent {
                        file_id,
                        line_id,
                        parent,
                    })?);
                }
            }

            // Update the child
            let current_child = current_graph.0.get(&line_id).map(|line| &line.child);
            if current_child != Some(&line.child) {
                for child in line.child.iter().copied() {
                    result.extend(self.svg_diff(ChangeContent::LineChild {
                        file_id,
                        line_id,
                        child,
                    })?);
                }
            }

            // Update the content
            let current_content = current_graph.0.get(&line_id).map(|line| &line.content);
            if current_content != Some(&line.content) {
                for content in line.content.iter().copied() {
                    result.extend(self.svg_diff(ChangeContent::LineContent {
                        file_id,
                        line_id,
                        content,
                    })?);
                }
            }
        }

        // Returns the changes
        Ok(result)
    }

    /// Applies the given [`Change`] to the repository and returns the hash of
    /// the applied change.
    ///
    /// If the [Change] is already applied, `Ok(())` will be returned and
    /// nothing will be done.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn apply(&mut self, change: Change) -> Result<ChangeHash, Self::Error>;

    /// Unapplies the change with the given [`ChangeHash`].
    ///
    /// If the change is not applied, `Ok(())` will be returned and nothing
    /// will be done.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn unapply(&mut self, change_hash: ChangeHash) -> Result<(), Self::Error>;

    /// Commit the changes made to the repository.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn commit(self) -> Result<(), Self::Error>;
}

/// A graph that contains the state of a file in the repository.
///
/// This graph represent a particular state of an OVG but the missing links are
/// also stored. For example if in the repository, a line `A` has a child `B`
/// but `B` don't have `A` as a parent, this link will be added to this graph.
pub struct FileGraph(HashMap<LineId, FileLine>);

/// A line in a [`FileGraph`].
struct FileLine {
    /// The parent of the line.
    parent: HashSet<LineId>,

    /// The child of the line.
    child: HashSet<LineId>,

    /// The content of the line.
    content: HashSet<ContentHash>,
}

/// A graph that contains the state of a file in the repository.
///
/// This graph does not contain any cycles. This graph is made from a
/// [`FileGraph`] using the Tarjan's algorithm.
struct AcyclicFileGraph<'graph>(HashMap<LineId, Vec<&'graph FileLine>>);

impl<'graph> AcyclicFileGraph<'graph> {
    /// An implementation of Tarjan's algorithm.
    ///
    /// For more information,
    /// see <https://en.wikipedia.org/wiki/Tarjan%27s_strongly_connected_components_algorithm>
    fn tarjan_search(
        &mut self,
        current: LineId,
        graph: &'graph FileGraph,
        last_identifier: &mut usize,
        identifiers: &mut HashMap<LineId, Option<usize>>,
        stack: &mut Vec<LineId>,
        lowlink_values: &mut HashMap<LineId, usize>,
    ) {
        // Add the current line to the stack
        stack.push(current);

        // Set the identifier and the low link value
        identifiers.insert(current, Some(*last_identifier));
        lowlink_values.insert(current, *last_identifier);

        *last_identifier = {
            #[expect(
                clippy::expect_used,
                reason = "the computer can't have any memory to store all the lines"
            )]
            last_identifier.checked_add(1).expect("too many lines")
        };

        // Iterate over all neighbors of the current line
        #[expect(
            clippy::indexing_slicing,
            reason = "current line is always in the graph"
        )]
        for &next in &graph.0[&current].child {
            // If the neighbor is not visited, visit it
            if !identifiers.contains_key(&next) {
                self.tarjan_search(
                    next,
                    graph,
                    last_identifier,
                    identifiers,
                    stack,
                    lowlink_values,
                );
            }

            // If the neighbor is on the stack, update the low link value
            if stack.contains(&next) {
                lowlink_values.insert(current, lowlink_values[&current].min(lowlink_values[&next]));
            }
        }

        // If the current line is the root of a SCC
        if identifiers[&current] == Some(lowlink_values[&current]) {
            let mut result = HashMap::new();
            while let Some(line) = stack.pop() {
                #[expect(
                    clippy::indexing_slicing,
                    reason = "all values in the stack come from the graph"
                )]
                result.insert(line, &graph.0[&line]);
                if line == current {
                    break;
                }
            }
            if result.contains_key(&LineId::FIRST) {
                self.0.insert(LineId::FIRST, result.into_values().collect());
            } else if result.contains_key(&LineId::LAST) {
                self.0.insert(LineId::LAST, result.into_values().collect());
            } else {
                self.0.insert(current, result.into_values().collect());
            }
        }
    }
}

impl<'graph> From<&'graph FileGraph> for AcyclicFileGraph<'graph> {
    fn from(value: &'graph FileGraph) -> Self {
        let mut last_identifier = 0;
        let mut identifiers = HashMap::new();
        let mut stack = Vec::new();
        let mut lowlink_values = HashMap::new();

        // Build the graph
        let mut graph = AcyclicFileGraph(HashMap::new());
        graph.tarjan_search(
            LineId::FIRST,
            value,
            &mut last_identifier,
            &mut identifiers,
            &mut stack,
            &mut lowlink_values,
        );
        graph
    }
}
