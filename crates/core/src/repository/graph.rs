//! Implement a trait extention that add function to work with file graphs.

use std::collections::{BTreeSet, HashSet};
use std::ops::Deref;

use petgraph::prelude::{DiGraphMap, Direction};

use super::Repository;
use super::diff::DiffExt;
use super::head::HeadExt;
use crate::change::{Change, ChangeContent, FileId, LineId};

/// A graph that represent a file in a repository.
pub struct FileGraph(DiGraphMap<LineId, ()>);

impl Deref for FileGraph {
    type Target = DiGraphMap<LineId, ()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A trait extention that add function to work with file graphs.
pub trait GraphExt<'manager>: Repository<'manager> + HeadExt<'manager> + DiffExt<'manager> {
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
    /// This graph does not contain the content of the lines.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn file_graph(&self, file_id: FileId) -> Result<FileGraph, Self::Error> {
        let mut current = BTreeSet::from_iter(self.existing_lines(file_id)?);
        let mut graph = DiGraphMap::with_capacity(current.len(), current.len());
        while let Some(line_id) = current.pop_first() {
            for parent in self.line_parent(file_id, line_id)? {
                if !graph.contains_node(parent) && !current.contains(&parent) {
                    current.insert(parent);
                }
                graph.add_edge(parent, line_id, ());
            }
            for child in self.line_child(file_id, line_id)? {
                if !graph.contains_node(child) && !current.contains(&child) {
                    current.insert(child);
                }
                graph.add_edge(line_id, child, ());
            }
        }
        Ok(FileGraph(graph))
    }

    /// Returns a list of [Change] that transform a file of the [Repository]
    /// into the given [`FileGraph`].
    ///
    /// This function does not modify the repository.
    ///
    /// The changes returned does not contain changes for the content of the
    /// lines.
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
        for line_id in current_graph.nodes() {
            if !graph.contains_node(line_id) {
                result.extend(self.svg_diff(ChangeContent::LineExistence {
                    file_id,
                    line_id,
                    existence: false,
                })?);
            }
        }

        // Add all the lines that are in the graph but not in the repository
        for line_id in graph.nodes() {
            if !current_graph.contains_node(line_id) {
                result.extend(self.svg_diff(ChangeContent::LineExistence {
                    file_id,
                    line_id,
                    existence: true,
                })?);
            }
        }

        // Update the links and content for each line of the graph
        for line_id in graph.nodes() {
            // Update the parent
            let current_parents = current_graph
                .neighbors_directed(line_id, Direction::Incoming)
                .collect::<Vec<_>>();
            let graph_parents = graph
                .neighbors_directed(line_id, Direction::Incoming)
                .collect::<Vec<_>>();
            if current_parents != graph_parents {
                for parent in graph_parents {
                    result.extend(self.svg_diff(ChangeContent::LineParent {
                        file_id,
                        line_id,
                        parent,
                    })?);
                }
            }

            // Update the child
            let current_children = current_graph
                .neighbors_directed(line_id, Direction::Outgoing)
                .collect::<Vec<_>>();
            let graph_children = graph
                .neighbors_directed(line_id, Direction::Outgoing)
                .collect::<Vec<_>>();
            if current_children != graph_children {
                for child in graph_children {
                    result.extend(self.svg_diff(ChangeContent::LineChild {
                        file_id,
                        line_id,
                        child,
                    })?);
                }
            }
        }

        // Returns the changes
        Ok(result)
    }
}

impl<'manager, T: Repository<'manager>> GraphExt<'manager> for T {}
