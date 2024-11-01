//! Implement a trait extention that add function to work with file graphs.

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};

use super::Repository;
use super::diff::DiffExt;
use super::head::HeadExt;
use crate::change::{Change, ChangeContent, FileId, LineId};
use crate::registry::ContentHash;

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
}

impl<'manager, T: Repository<'manager>> GraphExt<'manager> for T {}

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
