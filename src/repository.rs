//! Defines a [`RepositoryManager`] and [Repository] traits.
//!
//! These traits are used to open repositories, apply changes to them and
//! retrieve information from them.

use core::mem::discriminant;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use similar::{Algorithm, DiffOp};
use thiserror::Error;
use uuid::Uuid;

use crate::change::{Change, ChangeContent, ChangeHash, FileId, LineId, SingleId};
use crate::registry::{ContentHash, Registry};
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
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "repo:{}", self.0)
    }
}

impl FromStr for RepositoryId {
    type Err = uuid::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("repo:").unwrap_or(value);
        Ok(Self(Uuid::parse_str(value)?))
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
        let mut graph: HashMap<LineId, FileLine> = HashMap::with_capacity(current.len());

        // Find all the lines in the graph
        while let Some(line_id) = current.pop_first() {
            let parents = self.line_parent(file_id, line_id)?;
            let children = self.line_child(file_id, line_id)?;
            let contents = self.line_content(file_id, line_id)?;

            // Search for the parents and children in the graph
            for parent in &parents {
                // Update the links in the graph
                let parent_line = graph.entry(*parent).or_insert_with(|| FileLine {
                    parent: HashSet::new(),
                    child: HashSet::new(),
                    content: HashSet::new(),
                });
                parent_line.child.insert(line_id);

                // If the line is unknown, add it to the lines to be processed
                if !graph.contains_key(parent) && !current.contains(parent) {
                    current.insert(*parent);
                }
            }
            for child in &children {
                // Update the links in the graph
                let child_line = graph.entry(*child).or_insert_with(|| FileLine {
                    parent: HashSet::new(),
                    child: HashSet::new(),
                    content: HashSet::new(),
                });
                child_line.parent.insert(line_id);

                // If the line is unknown, add it to the lines to be processed
                if !graph.contains_key(child) && !current.contains(child) {
                    current.insert(*child);
                }
            }

            // Insert the line in the graph
            match graph.entry(line_id) {
                Entry::Occupied(entry) => {
                    let line = entry.into_mut();
                    line.parent.extend(self.line_parent(file_id, line_id)?);
                    line.child.extend(self.line_child(file_id, line_id)?);
                    line.content.extend(self.line_content(file_id, line_id)?);
                    line
                }
                Entry::Vacant(entry) => entry.insert(FileLine {
                    parent: parents,
                    child: children,
                    content: contents,
                }),
            };
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

/// An error that can happen while rendering a [`LinearFile`] into a [Write].
#[derive(Debug, Error)]
pub enum LinearFileRenderError<Reg: Registry> {
    /// An error that can happen while reading from the registry.
    #[error("registry error: {0}")]
    Registry(Reg::Error),

    /// The given registry does not contain the given content.
    #[error("content not found in the registry: {0}")]
    NotFound(ContentHash),

    /// An io error.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// An error that can happen while parsing a [Read] into a [`LinearFile`].
#[derive(Debug, Error)]
pub enum LinearFileParseError<Reg: Registry> {
    /// An error that can happen while writing to the registry.
    #[error("registry error: {0}")]
    Registry(Reg::Error),

    /// An io error.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// A representation of a file in the repository in a linear way.
pub struct LinearFile(Vec<LinearFileLine>);

impl LinearFile {
    /// Render a [`LinearFile`] into a [Write].
    ///
    /// # Errors
    ///
    /// There can be an error if a content is not found in the registry or if
    /// there is an io error while writing to the writer or reading from the
    /// registry.
    pub fn render<Reg: Registry>(
        &self,
        regitry: impl AsRef<Reg>,
        mut writer: impl Write,
    ) -> Result<(), LinearFileRenderError<Reg>> {
        for (index, line) in self.0.iter().enumerate() {
            if index > 0 {
                writer.write_all(b"\n")?;
            }
            match *line {
                LinearFileLine::Line(_, content) => {
                    let mut content = regitry
                        .as_ref()
                        .read(content)
                        .map_err(LinearFileRenderError::Registry)?
                        .ok_or(LinearFileRenderError::NotFound(content))?;
                    io::copy(&mut content, &mut writer)?;
                }
                LinearFileLine::Conflict(id, ref possible_paths) => {
                    writeln!(writer, "<<<<<<< CONFLICT {id}")?;
                    for (index, lines) in possible_paths.iter().enumerate() {
                        if index > 0 {
                            writeln!(writer, "=======")?;
                        }
                        for &(_, content) in lines {
                            let mut content = regitry
                                .as_ref()
                                .read(content)
                                .map_err(LinearFileRenderError::Registry)?
                                .ok_or(LinearFileRenderError::NotFound(content))?;
                            io::copy(&mut content, &mut writer)?;
                            writeln!(writer)?;
                        }
                    }
                    write!(writer, ">>>>>>> CONFLICT")?;
                }
                LinearFileLine::Cycle(id, ref lines) => {
                    writeln!(writer, "<<<<<<< CYCLE {id}")?;
                    for &(_, content) in lines {
                        let mut content = regitry
                            .as_ref()
                            .read(content)
                            .map_err(LinearFileRenderError::Registry)?
                            .ok_or(LinearFileRenderError::NotFound(content))?;
                        io::copy(&mut content, &mut writer)?;
                        writeln!(writer)?;
                    }
                    write!(writer, ">>>>>>> CYCLE")?;
                }
            }
        }
        Ok(())
    }

    /// Parse a [Read] into a [`LinearFile`] and store new content in the
    /// registry.
    ///
    /// # Errors
    ///
    /// There can be an error if there is an io error while writing to the
    /// registry or if there is an io error while reading from the reader.
    pub fn parse<Reg: Registry>(
        regitry: impl AsRef<Reg>,
        reader: impl Read,
    ) -> Result<Self, LinearFileParseError<Reg>> {
        let mut result = Vec::new();

        // Parse all lines
        let mut reader = BufReader::new(reader);
        let mut line = vec![1];
        let mut conflict = None;
        let mut cycle = None;
        while !line.is_empty() {
            // Read the next line
            line.clear();
            reader.read_until(b'\n', &mut line)?;
            let content = line.strip_suffix(b"\n").unwrap_or(&line);

            // Check if we have a conflict
            if let Some(id) = content.strip_prefix(b"<<<<<<< CONFLICT ") {
                if conflict.is_none() && cycle.is_none() {
                    if let Ok(id) = Uuid::try_parse_ascii(id) {
                        conflict = Some((id, vec![Vec::new()]));
                    }
                    continue;
                }
            }

            // Check if we change path in a conflict
            if content == b"=======" {
                if let Some((_, ref mut paths)) = conflict {
                    paths.push(Vec::new());
                    continue;
                }
            }

            // Check if we end a conflict
            if content == b">>>>>>> CONFLICT" {
                if let Some((id, paths)) = conflict.take() {
                    result.push(LinearFileLine::Conflict(id, paths));
                    continue;
                }
            }

            // Check if we have a cycle
            if let Some(id) = content.strip_prefix(b"<<<<<<< CYCLE ") {
                if conflict.is_none() && cycle.is_none() {
                    if let Ok(id) = Uuid::try_parse_ascii(id) {
                        cycle = Some((id, Vec::new()));
                    }
                    continue;
                }
            }

            // Check if we end a cycle
            if content == b">>>>>>> CYCLE" {
                if let Some((id, lines)) = cycle.take() {
                    result.push(LinearFileLine::Cycle(id, lines));
                    continue;
                }
            }

            // If we don't have a conflict or a cycle, we have a line
            let content = regitry
                .as_ref()
                .write(content)
                .map_err(LinearFileParseError::Registry)?;
            if let Some((_, ref mut paths)) = conflict {
                // SAFETY: `paths` is never empty
                unsafe {
                    paths
                        .last_mut()
                        .unwrap_unchecked()
                        .push((LineId::UNKNOWN, content));
                }
            } else if let Some((_, ref mut lines)) = cycle {
                lines.push((LineId::UNKNOWN, content));
            } else {
                result.push(LinearFileLine::Line(LineId::UNKNOWN, content));
            }
        }

        // If a conflict is still open, we add it as normal lines
        if let Some((id, paths)) = conflict {
            // Add the conflict start
            let content = regitry
                .as_ref()
                .write(format!("<<<<<<< CONFLICT {id}").as_bytes())
                .map_err(LinearFileParseError::Registry)?;
            result.push(LinearFileLine::Line(LineId::UNKNOWN, content));

            // Add the paths
            for (index, path) in paths.into_iter().enumerate() {
                // Add the path separator
                if index > 0 {
                    let content = regitry
                        .as_ref()
                        .write(format!("======={path:?}").as_bytes())
                        .map_err(LinearFileParseError::Registry)?;
                    result.push(LinearFileLine::Line(LineId::UNKNOWN, content));
                }

                // Add all the lines
                for (id, content) in path {
                    result.push(LinearFileLine::Line(id, content));
                }
            }
        }

        // If a cycle is still open, we add it as normal lines
        if let Some((id, lines)) = cycle {
            // Add the cycle start
            let content = regitry
                .as_ref()
                .write(format!("<<<<<<< CYCLE {id}").as_bytes())
                .map_err(LinearFileParseError::Registry)?;
            result.push(LinearFileLine::Line(LineId::UNKNOWN, content));

            // Add all the lines
            for (id, content) in lines {
                result.push(LinearFileLine::Line(id, content));
            }
        }

        // Return the parsed file
        Ok(Self(result))
    }

    /// Populate the ids of all lines in the file using the given
    /// [`LinearFile`] using the Patience diff algorithm.
    pub fn populate_ids(&mut self, before: &Self) {
        let ops = similar::capture_diff_slices(Algorithm::Patience, &before.0, &self.0);
        for op in ops {
            if let DiffOp::Equal {
                old_index,
                new_index,
                len,
            } = op
            {
                for offset in 0..len {
                    if let (
                        Some(LinearFileLine::Line(old_id, _)),
                        Some(LinearFileLine::Line(id, _)),
                    ) = (
                        before.0.get(old_index.saturating_add(offset)),
                        self.0.get_mut(new_index.saturating_add(offset)),
                    ) {
                        *id = *old_id;
                    }
                }
            }
        }
    }
}

/// A line in a [`LinearFile`].
#[derive(Debug, Clone, Eq, PartialOrd, Ord)]
pub enum LinearFileLine {
    /// A normal line.
    Line(LineId, ContentHash),

    /// A conflict that contains all possible paths in the graph.
    ///
    /// Every conflict has a unique ID that is used to identify it. This ID
    /// should not be modified by the user in any way. The content of the
    /// conflict should not be modified too, the only way to change it is by
    /// removing it.
    Conflict(Uuid, Vec<Vec<(LineId, ContentHash)>>),

    /// A cycle that contains all lines in the cycle.
    ///
    /// Every cycle has a unique ID that is used to identify it. This ID
    /// should not be modified by the user in any way. The content of the
    /// cycle should not be modified too, the only way to change it is by
    /// removing it.
    Cycle(Uuid, Vec<(LineId, ContentHash)>),
}

impl PartialEq for LinearFileLine {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Line(_, first), Self::Line(_, other)) => first == other,
            (Self::Conflict(first, _), Self::Conflict(other, _))
            | (Self::Cycle(first, _), Self::Cycle(other, _)) => first == other,
            _ => false,
        }
    }
}

impl Hash for LinearFileLine {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Line(_, content) => content.hash(state),
            Self::Conflict(id, _) | Self::Cycle(id, _) => id.hash(state),
        }
    }
}
