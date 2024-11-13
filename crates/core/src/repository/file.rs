use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;

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
use crate::registry::ContentHash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileLine(LineId, ContentHash);
pub type CycleLine = Vec<FileLine>;
pub type ConflictPath = Vec<CycleLine>;
pub type Conflict = Vec<ConflictPath>;

pub struct File(Vec<Conflict>);

fn render_graph<'manager, Repo: Repository<'manager>>(
    repository: &Repo,
    file_id: FileId,
) -> Result<DiGraphMap<FileLine, ()>, Repo::Error> {
    let mut current = VecDeque::from_iter(repository.existing_lines(file_id)?);
    let mut visited = HashSet::with_capacity(current.len());
    let mut graph = DiGraphMap::with_capacity(current.len(), current.len());
    while let Some(line_id) = current.pop_front() {
        visited.insert(line_id);
        for content in repository.line_content(file_id, line_id)? {
            graph.add_node(FileLine(line_id, content));
            for parent in repository.line_parent(file_id, line_id)? {
                if !visited.contains(&parent) && !current.contains(&parent) {
                    current.push_back(parent);
                }
                for other_content in repository.line_content(file_id, parent)? {
                    graph.add_edge(
                        FileLine(parent, other_content),
                        FileLine(line_id, content),
                        (),
                    );
                }
            }
            for child in repository.line_child(file_id, line_id)? {
                if !visited.contains(&child) && !current.contains(&child) {
                    current.push_back(child);
                }
                for other_content in repository.line_content(file_id, child)? {
                    graph.add_edge(
                        FileLine(line_id, content),
                        FileLine(child, other_content),
                        (),
                    );
                }
            }
        }
    }
    Ok(graph)
}

fn make_linear(graph: &DiGraph<Vec<FileLine>, ()>) -> Vec<Vec<NodeIndex>> {
    let mut lines = Vec::with_capacity(graph.node_count());
    let mut visited = HashSet::with_capacity(graph.node_count());
    let mut current = graph
        .node_indices()
        .filter(|&i| graph.neighbors_directed(i, Direction::Incoming).count() == 0)
        .collect::<VecDeque<_>>();
    let mut current_conflict = Vec::new();
    while let Some(i) = current.pop_front() {
        // Only visit the node of all its parents have been visited
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

fn flatten_conflict(graph: &DiGraph<Vec<FileLine>, ()>, lines: &Vec<Vec<NodeIndex>>) -> File {
    let mut file = File(Vec::with_capacity(lines.len()));
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
            let last = path.last().unwrap();
            let children = graph
                .neighbors(*last)
                .filter(|i| line.contains(i))
                .collect::<Vec<_>>();
            if children.is_empty() {
                paths.push(path.into_iter().map(|i| graph[i].clone()).collect());
            } else {
                let last_index = children.len() - 1;
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
        file.0.push(paths);
    }
    file
}

fn unflatten_conflicts(file: &File) -> DiGraph<&Vec<FileLine>, ()> {
    let mut graph = DiGraph::with_capacity(file.0.len(), file.0.len());
    let mut mapping = HashMap::new();
    for conflict_paths in file.0.iter() {
        for path in conflict_paths.iter() {
            for line in path.iter() {
                mapping.entry(line).or_insert_with(|| graph.add_node(line));
            }
        }
    }
    let mut last_nodes = HashSet::new();
    for conflict_paths in file.0.iter() {
        let mut new_last_nodes = HashSet::with_capacity(conflict_paths.len());
        for path in conflict_paths.iter() {
            let mut last_nodes = last_nodes.clone();
            for line in path.iter() {
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
pub enum FileDiffError<RepoError: Error> {
    #[error("repository error: {0}")]
    Repository(#[from] RepoError),

    #[error("cycle must be resolved to create a file diff")]
    Cycle,
}

fn remove_cycles<RepoError: Error>(
    graph: DiGraph<&Vec<FileLine>, ()>,
) -> Result<DiGraphMap<FileLine, ()>, FileDiffError<RepoError>> {
    let mut new_graph = DiGraphMap::with_capacity(graph.node_count(), graph.node_count());
    for i in graph.node_indices() {
        let lines = graph[i];
        if lines.len() != 1 {
            return Err(FileDiffError::Cycle);
        }
        new_graph.add_node(lines[0]);
    }
    for i in graph.node_indices() {
        for j in graph.neighbors(i) {
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

pub trait GraphExt<'manager>: Repository<'manager> + HeadExt<'manager> + Sized {
    fn render(&self, file_id: FileId) -> Result<File, Self::Error> {
        let graph = render_graph(self, file_id)?;
        let graph = condensation(graph.into_graph::<DefaultIx>(), true);
        let lines = make_linear(&graph);
        Ok(flatten_conflict(&graph, &lines))
    }

    fn file_diff(
        &self,
        file_id: FileId,
        target: &File,
    ) -> Result<HashSet<Change>, FileDiffError<Self::Error>> {
        let current_graph = render_graph(self, file_id)?;
        let current_acyclic = condensation(current_graph.into_graph::<DefaultIx>(), true);
        let current_lines = make_linear(&current_acyclic);
        let current_file = flatten_conflict(&current_acyclic, &current_lines);

        // TODO: Populate IDS

        let graph = unflatten_conflicts(target);
        let graph = remove_cycles(graph)?;
        let mut lines: HashMap<LineId, HashSet<ContentHash>> =
            HashMap::with_capacity(graph.node_count());
        for line in graph.nodes() {
            lines.entry(line.0).or_default().insert(line.1);
        }

        let current_graph = render_graph(self, file_id)?;
        let mut current_lines: HashMap<LineId, HashSet<ContentHash>> =
            HashMap::with_capacity(current_graph.node_count());
        for line in current_graph.nodes() {
            current_lines.entry(line.0).or_default().insert(line.1);
        }

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
        for (line_id, contents) in lines.iter() {
            if current_lines.get(line_id) != Some(contents) {
                for content in contents.iter() {
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
                        .neighbors_directed(FileLine(line_id, *content), Direction::Incoming)
                        .map(|parent| parent.0)
                })
                .collect::<HashSet<_>>();
            let parents = contents
                .iter()
                .flat_map(|content| {
                    graph
                        .neighbors_directed(FileLine(line_id, *content), Direction::Incoming)
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
                        .neighbors_directed(FileLine(line_id, *content), Direction::Outgoing)
                        .map(|child| child.0)
                })
                .collect::<HashSet<_>>();
            let children = contents
                .iter()
                .flat_map(|content| {
                    graph
                        .neighbors_directed(FileLine(line_id, *content), Direction::Outgoing)
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

        Ok(changes)
    }
}
