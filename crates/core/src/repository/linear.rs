//! Implement a representation of a file in the repository in a linear way.

use core::mem::discriminant;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::ops::Deref;

use petgraph::Direction;
use petgraph::algo::tarjan_scc;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::Visitable;
use similar::{Algorithm, DiffOp};
use thiserror::Error;
use uuid::Uuid;

use super::Repository;
use super::graph::FileGraph;
use crate::change::LineId;
use crate::registry::{ContentHash, Registry};

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

    /// Generate a [`LinearFile`] from the given graph.
    #[must_use]
    pub fn from_graph(graph: &FileGraph) -> TempLinearFile {
        // Make an acyclic graph from the graph
        let sccs = tarjan_scc(&**graph);
        let mut acyclic_graph =
            DiGraphMap::<CycleLine, ()>::with_capacity(sccs.len(), graph.edge_count());
        let mut mapping = HashMap::with_capacity(graph.node_count());
        for lines in sccs {
            let cycle_line_id = if lines.len() == 1 {
                #[expect(clippy::indexing_slicing, reason = "the length is checked to be 1")]
                CycleLine::Line(lines[0])
            } else {
                CycleLine::Cycle(LineId::combine(lines.iter().copied()))
            };
            acyclic_graph.add_node(cycle_line_id);
            for node in lines {
                mapping.insert(node, cycle_line_id);
            }
        }

        #[expect(
            clippy::indexing_slicing,
            reason = "first node is always present in a file graph"
        )]
        let first_id = mapping[&LineId::FIRST];
        for (line_a, line_b, ()) in graph.all_edges() {
            #[expect(clippy::indexing_slicing, reason = "all nodes are in the mapping")]
            acyclic_graph.add_edge(mapping[&line_a], mapping[&line_b], ());
        }
        drop(mapping);

        // Detect conflicts
        let mut lines = Vec::with_capacity(acyclic_graph.node_count());
        let mut visited_lines = HashSet::with_capacity(acyclic_graph.node_count());
        let mut current_conflict = Vec::new();
        let mut current = VecDeque::from_iter([first_id]);
        'outer: while let Some(node) = current.pop_front() {
            // Only visit the node if all parents have been visited
            for parent in acyclic_graph.neighbors_directed(node, Direction::Incoming) {
                if !current.contains(&parent) && !visited_lines.contains(&parent) {
                    current.push_back(parent);
                    continue 'outer;
                }
            }

            // If we are at the end of a conflict or a simple line
            if current.is_empty() {
                if !current_conflict.is_empty() {
                    let mut conflict =
                        DiGraphMap::with_capacity(current_conflict.len(), current_conflict.len());
                    for line in current_conflict.iter().copied() {
                        conflict.add_node(line);
                        for child in acyclic_graph.neighbors_directed(line, Direction::Outgoing) {
                            if current_conflict.contains(&child) {
                                conflict.add_edge(line, child, ());
                            }
                        }
                        visited_lines.insert(line);
                    }
                    lines.push(ConflictLine::Conflict(conflict));
                    current_conflict.clear();
                }
                lines.push(ConflictLine::Line(node));
                visited_lines.insert(node);
            } else {
                current_conflict.push(node);
            }

            // Visit the children
            for child in acyclic_graph.neighbors_directed(node, Direction::Outgoing) {
                if !current.contains(&child) {
                    current.push_back(child);
                }
            }
        }

        // Return the generated file
        TempLinearFile(lines)
    }
}

/// A temporary representation of a file in the repository in a linear way.
pub struct TempLinearFile(Vec<ConflictLine>);

/// A data structure to store a line or cycle in the graph.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CycleLine {
    /// A cycle of lines.
    Cycle(Uuid),

    /// A line.
    Line(LineId),
}

/// A data structure to store a line or a conflict.
pub enum ConflictLine {
    /// A conflict.
    Conflict(DiGraphMap<CycleLine, ()>),

    /// A line.
    Line(CycleLine),
}

/// A recursive function used by [`conflict_paths`] to get all the possible
/// paths in a conflict.
fn visit_path(graph: &DiGraphMap<CycleLine, ()>, paths: &mut Vec<Vec<CycleLine>>, node: CycleLine) {
    let path_index = paths.len() - 1;
    paths[path_index].push(node);
    let path_len = paths[path_index].len();
    for (i, child) in graph
        .neighbors_directed(node, Direction::Outgoing)
        .enumerate()
    {
        if i != 0 {
            paths.push(paths[path_index][..path_len].to_vec());
        }
        visit_path(graph, paths, child);
    }
}

/// Get all the possible paths in a conflict.
fn conflict_paths(graph: &DiGraphMap<CycleLine, ()>) -> Vec<Vec<CycleLine>> {
    let mut paths = Vec::new();
    let starting_nodes = graph
        .nodes()
        .filter(|node| {
            graph
                .neighbors_directed(*node, Direction::Incoming)
                .next()
                .is_none()
        })
        .collect::<Vec<_>>();
    for node in starting_nodes {
        paths.push(Vec::new());
        visit_path(graph, &mut paths, node);
    }
    paths
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
