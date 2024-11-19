//! This module defines a trait extention for a [Repository] to work with files
//! in a repository.

use core::mem::discriminant;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};

use petgraph::Direction;
use petgraph::algo::condensation;
use petgraph::csr::DefaultIx;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::prelude::DiGraphMap;
use solipr_stack::StackVec;
use thiserror::Error;

use super::Repository;
use super::head::HeadExt;
use crate::change::{Change, ChangeContent, FileId, LineId};
use crate::registry::{ContentHash, Registry};

/// Represents a line in a temporary file, consisting of a line identifier and
/// its content hash.
type TempFileLine = (LineId, ContentHash);

/// Represents a cycle line, which is a collection of temporary file lines.
type CycleLine = Vec<TempFileLine>;

/// Represents a conflict path, composed of multiple cycle lines.
type ConflictPath = Vec<CycleLine>;

/// Represents a conflict, which consists of several conflict paths.
type Conflict = Vec<ConflictPath>;

/// Represents a temporary file, containing a list of conflicts.
type TempFile = Vec<Conflict>;

/// Constructs a directed graph representing the relationships between lines
/// in a file within a repository.
///
/// The graph nodes consist of pairs of line identifiers and their content
/// hashes, and edges represent the parent-child relationships between lines.
///
/// # Errors
///
/// The function returns a `Repo::Error` if there was a problem while reading
/// the data from the repository.
fn render_graph<'manager, Repo: Repository<'manager>>(
    repository: &Repo,
    file_id: FileId,
) -> Result<DiGraphMap<TempFileLine, ()>, Repo::Error> {
    let mut current = VecDeque::from_iter(repository.existing_lines(file_id)?);
    let mut visited = HashSet::with_capacity(current.len());
    let mut graph = DiGraphMap::with_capacity(current.len(), current.len());
    while let Some(line_id) = current.pop_front() {
        visited.insert(line_id);
        for content in repository.line_content(file_id, line_id)? {
            graph.add_node((line_id, content));
            for parent in repository.line_parent(file_id, line_id)? {
                if !visited.contains(&parent) && !current.contains(&parent) {
                    current.push_back(parent);
                }
                for other_content in repository.line_content(file_id, parent)? {
                    graph.add_edge((parent, other_content), (line_id, content), ());
                }
            }
            for child in repository.line_child(file_id, line_id)? {
                if !visited.contains(&child) && !current.contains(&child) {
                    current.push_back(child);
                }
                for other_content in repository.line_content(file_id, child)? {
                    graph.add_edge((line_id, content), (child, other_content), ());
                }
            }
        }
    }
    Ok(graph)
}

/// Transforms a directed graph into a linear sequence of node indices,
/// representing the relationships between lines in a file.
///
/// The function identifies nodes with no incoming edges as starting points,
/// traverses through the graph, and constructs a linear representation of the
/// graph where each sequence of nodes represents a potential conflict.
///
/// # Returns
///
/// A vector of vectors, where each inner vector is a sequence of node indices
/// representing a conflict in the graph. If the inner vector only contains one
/// node, it is not a conflict but simply a line.
fn make_linear(graph: &DiGraph<Vec<TempFileLine>, ()>) -> Vec<Vec<NodeIndex>> {
    let mut lines = Vec::with_capacity(graph.node_count());
    let mut visited = HashSet::with_capacity(graph.node_count());
    let mut current = graph
        .node_indices()
        .filter(|&i| graph.neighbors_directed(i, Direction::Incoming).count() == 0)
        .collect::<VecDeque<_>>();
    let mut current_conflict = Vec::new();
    while let Some(i) = current.pop_front() {
        // Only visit the node if all its parents have been visited
        if graph
            .neighbors_directed(i, Direction::Incoming)
            .any(|j| !visited.contains(&j))
        {
            current.push_back(i);
            continue;
        }
        visited.insert(i);

        // If we are at the end of a conflict or a simple line
        if current.is_empty() {
            if !current_conflict.is_empty() {
                lines.push(current_conflict.clone());
                current_conflict.clear();
            }
            lines.push(vec![i]);
        } else {
            current_conflict.push(i);
        }

        // Visit all its children
        for j in graph.neighbors(i) {
            current.push_back(j);
        }
    }
    lines
}

/// Transforms a linear sequence of node indices, generated by `make_linear`,
/// into a temporary file representation.
///
/// The function processes each line of node indices, identifies paths through
/// the graph that correspond to potential conflicts, and constructs a
/// temporary file where each conflict is represented by multiple paths.
///
/// # Returns
///
/// A `TempFile` where each element represents a conflict, consisting of
/// multiple paths derived from the graph.
fn flatten_conflict(
    graph: &DiGraph<Vec<TempFileLine>, ()>,
    lines: &Vec<Vec<NodeIndex>>,
) -> TempFile {
    let mut file = Vec::with_capacity(lines.len());
    for line in lines {
        let mut paths = Conflict::new();
        let mut to_visit = line
            .iter()
            .copied()
            .filter(|&i| {
                graph
                    .neighbors_directed(i, Direction::Incoming)
                    .all(|j| !line.contains(&j))
            })
            .map(|i| {
                let mut path = Vec::with_capacity(line.len());
                path.push(i);
                path
            })
            .collect::<VecDeque<_>>();
        while let Some(mut path) = to_visit.pop_front() {
            #[expect(
                clippy::unwrap_used,
                reason = "path contains at least one element if it is a graph from the \
                          render_graph function"
            )]
            let last = path.last().unwrap();
            let children = graph
                .neighbors(*last)
                .filter(|i| line.contains(i))
                .collect::<Vec<_>>();
            if children.is_empty() {
                paths.push(path.into_iter().map(|i| graph[i].clone()).collect());
            } else {
                let last_index = children.len().saturating_sub(1);
                for (i, child) in children.into_iter().enumerate() {
                    if i == last_index {
                        path.push(child);
                    } else {
                        let mut new_path = path.clone();
                        new_path.push(child);
                        to_visit.push_back(new_path);
                    }
                }
                to_visit.push_back(path);
            }
        }
        file.push(paths);
    }
    file
}

/// Transforms a temporary file representation of conflicts into a directed
/// graph. It is the inverse of [`flatten_conflict`].
fn unflatten_conflicts(file: &TempFile) -> DiGraph<&Vec<TempFileLine>, ()> {
    let mut graph = DiGraph::with_capacity(file.len(), file.len());
    let mut mapping = HashMap::new();

    // Map each line in the conflict paths to a node in the graph
    for conflict_paths in file {
        for path in conflict_paths {
            for line in path {
                mapping.entry(line).or_insert_with(|| graph.add_node(line));
            }
        }
    }

    let mut last_nodes = HashSet::new();

    // Create edges between nodes to represent the sequence of lines in each path
    for conflict_paths in file {
        let mut new_last_nodes = HashSet::with_capacity(conflict_paths.len());
        for path in conflict_paths {
            let mut last_nodes = last_nodes.clone();
            for line in path {
                #[expect(clippy::indexing_slicing, reason = "all lines are in the mapping")]
                let line = mapping[line];
                for last_node in last_nodes.drain() {
                    if !graph.contains_edge(last_node, line) {
                        graph.add_edge(last_node, line, ());
                    }
                }
                last_nodes.insert(line);
            }
            new_last_nodes.extend(last_nodes);
        }
        last_nodes = new_last_nodes;
    }
    graph
}

#[derive(Debug, Error)]
/// An error that can occur while computing a file diff.
pub enum FileDiffError<RegError: Error, RepoError> {
    /// An error occurred while using the registry.
    #[error("registry error: {0}")]
    Registry(RegError),

    /// An error occurred while using the repository.
    #[error("repository error: {0}")]
    Repository(#[from] RepoError),

    /// The graph contains a cycle.
    #[error("cycle must be resolved to create a file diff")]
    Cycle,
}

/// Remove the cycles abstraction in the graph, if there is a cycle in the
/// graph, an error is returned.
fn remove_cycles<RegError: Error, RepoError: Error>(
    graph: &DiGraph<&Vec<TempFileLine>, ()>,
) -> Result<DiGraphMap<TempFileLine, ()>, FileDiffError<RegError, RepoError>> {
    let mut new_graph = DiGraphMap::with_capacity(graph.node_count(), graph.node_count());
    for i in graph.node_indices() {
        let lines = graph[i];
        if lines.len() != 1 {
            return Err(FileDiffError::Cycle);
        }
        #[expect(clippy::indexing_slicing, reason = "the length of lines is 1")]
        new_graph.add_node(lines[0]);
    }
    for i in graph.node_indices() {
        for j in graph.neighbors(i) {
            #[expect(
                clippy::indexing_slicing,
                reason = "i and j are in the graph because they come from the neighbors function"
            )]
            new_graph.add_edge(graph[i][0], graph[j][0], ());
        }
    }
    Ok(new_graph)
}

/// Returns the changes needed to replace the current value of an SVG.
///
/// # Errors
///
/// An error will be returned if there was an error while doing the
/// operation.
fn svg_diff<'manager, Repo: Repository<'manager>>(
    repository: &Repo,
    new_content: ChangeContent,
) -> Result<HashSet<Change>, Repo::Error> {
    let mut result = HashSet::new();
    let mut heads = Vec::from_iter(repository.heads(new_content.single_id())?);
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

/// A trait extention that add functions to work with file graphs.
pub trait FileExt<'manager>: Repository<'manager> + HeadExt<'manager> + Sized {
    /// Render a file graph into a linear file.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn render(&self, file_id: FileId) -> Result<File, Self::Error> {
        let graph = render_graph(self, file_id)?;
        let graph = condensation(graph.into_graph::<DefaultIx>(), true);
        let lines = make_linear(&graph);
        Ok(File::from(&flatten_conflict(&graph, &lines)))
    }

    /// Computes the differences between the current repository file and a
    /// target file.
    ///
    /// # Returns
    ///
    /// Returns a set of changes that can be applied to the repository to
    /// make it match the target file.
    ///
    /// # Errors
    ///
    /// Returns an error if there's an issue with the registry or repository,
    /// or if the target file contains cycles.
    fn file_diff<Reg: Registry>(
        &self,
        registry: &Reg,
        file_id: FileId,
        target: &File,
    ) -> Result<HashSet<Change>, FileDiffError<Reg::Error, Self::Error>> {
        // Convert back the target file to a graph
        let target = target
            .to_temp_file(registry)
            .map_err(FileDiffError::Registry)?;
        let graph = unflatten_conflicts(&target);
        let graph = remove_cycles(&graph)?;
        let mut lines: HashMap<LineId, HashSet<ContentHash>> =
            HashMap::with_capacity(graph.node_count());
        for line in graph.nodes() {
            lines.entry(line.0).or_default().insert(line.1);
        }

        // Generate the graph of the file in the repository
        let current_graph = render_graph(self, file_id)?;
        let mut current_lines: HashMap<LineId, HashSet<ContentHash>> =
            HashMap::with_capacity(current_graph.node_count());
        for line in current_graph.nodes() {
            current_lines.entry(line.0).or_default().insert(line.1);
        }

        // Generate all changes needed to go from the current graph to the target
        let mut changes = HashSet::new();

        // Delete all the lines that are in the repository but not in the graph
        for line_id in current_lines.keys() {
            if !lines.contains_key(line_id) {
                changes.extend(svg_diff(self, ChangeContent::LineExistence {
                    file_id,
                    line_id: *line_id,
                    existence: false,
                })?);
            }
        }

        // Add all the lines that are in the graph but not in the repository
        for line_id in lines.keys() {
            if !current_lines.contains_key(line_id) {
                changes.extend(svg_diff(self, ChangeContent::LineExistence {
                    file_id,
                    line_id: *line_id,
                    existence: true,
                })?);
            }
        }

        // Update line contents
        for (line_id, contents) in &lines {
            if current_lines.get(line_id) != Some(contents) {
                for content in contents {
                    changes.extend(svg_diff(self, ChangeContent::LineContent {
                        file_id,
                        line_id: *line_id,
                        content: *content,
                    })?);
                }
            }
        }

        // Update the links for each line of the graph
        for (line_id, contents) in lines {
            // Update the parent
            let current_parents = contents
                .iter()
                .flat_map(|content| {
                    current_graph
                        .neighbors_directed((line_id, *content), Direction::Incoming)
                        .map(|parent| parent.0)
                })
                .collect::<HashSet<_>>();
            let parents = contents
                .iter()
                .flat_map(|content| {
                    graph
                        .neighbors_directed((line_id, *content), Direction::Incoming)
                        .map(|parent| parent.0)
                })
                .collect::<HashSet<_>>();
            if current_parents != parents {
                for parent in parents {
                    changes.extend(svg_diff(self, ChangeContent::LineParent {
                        file_id,
                        line_id,
                        parent,
                    })?);
                }
            }

            // Update the children
            let current_children = contents
                .iter()
                .flat_map(|content| {
                    current_graph
                        .neighbors_directed((line_id, *content), Direction::Outgoing)
                        .map(|child| child.0)
                })
                .collect::<HashSet<_>>();
            let children = contents
                .iter()
                .flat_map(|content| {
                    graph
                        .neighbors_directed((line_id, *content), Direction::Outgoing)
                        .map(|child| child.0)
                })
                .collect::<HashSet<_>>();
            if current_children != children {
                for child in children {
                    changes.extend(svg_diff(self, ChangeContent::LineChild {
                        file_id,
                        line_id,
                        child,
                    })?);
                }
            }
        }

        // Returns the changes
        Ok(changes)
    }
}

/// A line in a [File].
#[derive(Clone, Copy, Eq)]
pub enum FileLine {
    /// A normal line.
    Line(LineId, ContentHash),

    /// The start of a cycle marker.
    CycleStart,

    /// The end of a cycle marker.
    CycleEnd,

    /// The start of a conflict marker.
    ConflictStart,

    /// The separator of a conflict marker.
    ConflictSeparator,

    /// The end of a conflict marker.
    ConflictEnd,
}

impl PartialEq for FileLine {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Line(_, hash_a), Self::Line(_, hash_b)) => hash_a == hash_b,
            _ => discriminant(self) == discriminant(other),
        }
    }
}

impl Hash for FileLine {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        if let Self::Line(_, content) = self {
            content.hash(state);
        }
    }
}

/// An error that can happen when parsing a [File] from a [Read].
#[derive(Debug, Error)]
pub enum FileParseError<RegError> {
    /// An error that can happen while writing from the registry.
    #[error("registry error: {0}")]
    Registry(#[from] RegError),

    /// An io error.
    #[error("io error: {0}")]
    Io(io::Error),
}

/// An error that can happen when writing a [File] to a [Write].
#[derive(Debug, Error)]
pub enum FileWriteError<RegError> {
    /// An error that can happen while reading from the registry.
    #[error("registry error: {0}")]
    Registry(#[from] RegError),

    /// The given registry does not contain the given content.
    #[error("a content is missing in the registry: {0}")]
    Missing(ContentHash),

    /// An io error.
    #[error("io error: {0}")]
    Io(io::Error),
}

/// The representation of a file in the repository.
pub struct File(Vec<FileLine>);

impl File {
    /// The characters that are used to mark the start of a conflict.
    const CONFLICT_START_STRING: &'static str = "<<<<<<< CONFLICT";

    /// The characters that are used to mark the separator of a conflict.
    const CONFLICT_SEPARATOR_STRING: &'static str = "=======";

    /// The characters that are used to mark the end of a conflict.
    const CONFLICT_END_STRING: &'static str = ">>>>>>> CONFLICT";

    /// The characters that are used to mark the start of a cycle.
    const CYCLE_START_STRING: &'static str = "<<<<<<< CYCLE";

    /// The characters that are used to mark the end of a cycle.
    const CYCLE_END_STRING: &'static str = ">>>>>>> CYCLE";

    /// A function that add a line to the file.
    ///
    /// This is used to reduce code repetition in the [`Self::to_temp_file`]
    /// function.
    fn insert_line(
        file: &mut TempFile,
        conflict: &mut [ConflictPath],
        line_id: LineId,
        content: ContentHash,
    ) {
        if conflict.is_empty() {
            file.push(vec![vec![vec![(line_id, content)]]]);
        } else {
            #[expect(clippy::unwrap_used, reason = "conflict is not empty")]
            conflict.last_mut().unwrap().push(vec![(line_id, content)]);
        }
    }

    /// Convert a [File] to a [`TempFile`].
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while writing to the
    /// registry.
    fn to_temp_file<Reg: Registry>(&self, registry: &Reg) -> Result<TempFile, Reg::Error> {
        let mut file = Vec::with_capacity(self.0.len());
        let mut conflict: Vec<ConflictPath> = Vec::new();

        // Insert all the lines
        for line in &self.0 {
            match line {
                FileLine::Line(line_id, content) => {
                    Self::insert_line(&mut file, &mut conflict, *line_id, *content);
                }
                FileLine::ConflictStart => {
                    if conflict.is_empty() {
                        conflict.push(Vec::new());
                    } else {
                        Self::insert_line(
                            &mut file,
                            &mut conflict,
                            LineId::unique(),
                            registry.write(Self::CONFLICT_START_STRING.as_bytes())?,
                        );
                    }
                }
                FileLine::ConflictSeparator => {
                    if conflict.is_empty() {
                        Self::insert_line(
                            &mut file,
                            &mut conflict,
                            LineId::unique(),
                            registry.write(Self::CONFLICT_SEPARATOR_STRING.as_bytes())?,
                        );
                    } else {
                        conflict.push(Vec::new());
                    }
                }
                FileLine::ConflictEnd => {
                    if conflict.is_empty() {
                        Self::insert_line(
                            &mut file,
                            &mut conflict,
                            LineId::unique(),
                            registry.write(Self::CONFLICT_END_STRING.as_bytes())?,
                        );
                    } else {
                        file.push(conflict);
                        conflict = Vec::new();
                    }
                }
                FileLine::CycleStart => {
                    Self::insert_line(
                        &mut file,
                        &mut conflict,
                        LineId::unique(),
                        registry.write(Self::CYCLE_START_STRING.as_bytes())?,
                    );
                }
                FileLine::CycleEnd => {
                    Self::insert_line(
                        &mut file,
                        &mut conflict,
                        LineId::unique(),
                        registry.write(Self::CYCLE_END_STRING.as_bytes())?,
                    );
                }
            }
        }

        // If a conflict is not finished we insert the line as normal lines
        if !conflict.is_empty() {
            Self::insert_line(
                &mut file,
                &mut conflict,
                LineId::unique(),
                registry.write(Self::CONFLICT_START_STRING.as_bytes())?,
            );
            for (i, path) in conflict.into_iter().enumerate() {
                if i > 0 {
                    Self::insert_line(
                        &mut file,
                        &mut [],
                        LineId::unique(),
                        registry.write(Self::CONFLICT_SEPARATOR_STRING.as_bytes())?,
                    );
                }
                for line in path {
                    #[expect(clippy::indexing_slicing, reason = "line can't be empty")]
                    let line = line[0];
                    Self::insert_line(&mut file, &mut [], line.0, line.1);
                }
            }
        }

        // Returns the file
        Ok(file)
    }

    /// Parse a [File] from a reader.
    ///
    /// # Errors
    ///
    /// An error is returned if there was an error while reading from the
    /// reader, or if the file does not end with a newline.
    pub fn parse<Reg: Registry>(
        registry: &Reg,
        reader: impl Read,
    ) -> Result<Self, FileParseError<Reg::Error>> {
        let mut file = Vec::new();
        let mut reader = BufReader::new(reader);
        let mut line = vec![1];
        while !line.is_empty() {
            line.clear();
            reader
                .read_until(b'\n', &mut line)
                .map_err(FileParseError::Io)?;
            let content = line.strip_suffix(b"\n").unwrap_or(&line);
            if content == Self::CONFLICT_START_STRING.as_bytes() {
                file.push(FileLine::ConflictStart);
            } else if content == Self::CONFLICT_SEPARATOR_STRING.as_bytes() {
                file.push(FileLine::ConflictSeparator);
            } else if content == Self::CONFLICT_END_STRING.as_bytes() {
                file.push(FileLine::ConflictEnd);
            } else if content == Self::CYCLE_START_STRING.as_bytes() {
                file.push(FileLine::CycleStart);
            } else if content == Self::CYCLE_END_STRING.as_bytes() {
                file.push(FileLine::CycleEnd);
            } else {
                file.push(FileLine::Line(LineId::UNKNOWN, registry.write(content)?));
            }
        }
        Ok(Self(file))
    }

    /// Writes the contents of the [File] to the provided [Write].
    ///
    /// This function iterates over each line in the `File`, writing its
    /// content to the given writer. Special markers for conflicts and
    /// cycles are written as predefined strings.
    ///
    /// # Errors
    ///
    /// Returns an error if there's an issue reading from the registry or
    /// writing to the output.
    pub fn write<Reg: Registry>(
        &self,
        registry: &Reg,
        mut writer: impl Write,
    ) -> Result<(), FileWriteError<Reg::Error>> {
        for (i, line) in self.0.iter().enumerate() {
            if i > 0 {
                writer.write_all(b"\n").map_err(FileWriteError::Io)?;
            }
            match line {
                FileLine::Line(_, content) => {
                    let mut content = registry
                        .read(*content)?
                        .ok_or(FileWriteError::Missing(*content))?;
                    io::copy(&mut content, &mut writer).map_err(FileWriteError::Io)?;
                }
                FileLine::ConflictStart => {
                    writer
                        .write_all(Self::CONFLICT_START_STRING.as_bytes())
                        .map_err(FileWriteError::Io)?;
                }
                FileLine::ConflictSeparator => {
                    writer
                        .write_all(Self::CONFLICT_SEPARATOR_STRING.as_bytes())
                        .map_err(FileWriteError::Io)?;
                }
                FileLine::ConflictEnd => {
                    writer
                        .write_all(Self::CONFLICT_END_STRING.as_bytes())
                        .map_err(FileWriteError::Io)?;
                }
                FileLine::CycleStart => {
                    writer
                        .write_all(Self::CYCLE_START_STRING.as_bytes())
                        .map_err(FileWriteError::Io)?;
                }
                FileLine::CycleEnd => {
                    writer
                        .write_all(Self::CYCLE_END_STRING.as_bytes())
                        .map_err(FileWriteError::Io)?;
                }
            }
        }
        Ok(())
    }
}

impl From<&TempFile> for File {
    fn from(value: &TempFile) -> Self {
        let mut file = Vec::with_capacity(value.iter().flatten().flatten().flatten().count());
        for conflict_paths in value {
            if conflict_paths.len() > 1 {
                file.push(FileLine::ConflictStart);
            }
            for (i, path) in conflict_paths.iter().enumerate() {
                if i > 0 {
                    file.push(FileLine::ConflictSeparator);
                }
                for cycle in path {
                    if cycle.len() > 1 {
                        file.push(FileLine::CycleStart);
                    }
                    for line in cycle {
                        file.push(FileLine::Line(line.0, line.1));
                    }
                    if cycle.len() > 1 {
                        file.push(FileLine::CycleEnd);
                    }
                }
            }
            if conflict_paths.len() > 1 {
                file.push(FileLine::ConflictEnd);
            }
        }
        Self(file)
    }
}
