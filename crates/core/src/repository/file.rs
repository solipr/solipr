use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Deref;

use petgraph::Direction;
use petgraph::algo::{condensation, tarjan_scc, toposort};
use petgraph::csr::DefaultIx;
use petgraph::prelude::DiGraphMap;
use uuid::Uuid;

use super::Repository;
use super::head::HeadExt;
use crate::change::{Change, FileId, LineId};
use crate::registry::ContentHash;

pub type CycleLine = Vec<(LineId, ContentHash)>;
pub type ConflictPath = Vec<CycleLine>;
pub type Conflict = Vec<ConflictPath>;

pub struct File(Vec<Conflict>);

pub trait GraphExt<'manager>: Repository<'manager> + HeadExt<'manager> {
    fn render(&self, file_id: FileId) -> Result<File, Self::Error> {
        // Generate a graph from the file_id
        let mut current = VecDeque::from_iter(self.existing_lines(file_id)?);
        let mut visited = HashSet::with_capacity(current.len());
        let mut graph = DiGraphMap::with_capacity(current.len(), current.len());
        while let Some(line_id) = current.pop_front() {
            visited.insert(line_id);
            for content in self.line_content(file_id, line_id)? {
                for parent in self.line_parent(file_id, line_id)? {
                    if !visited.contains(&parent) && !current.contains(&parent) {
                        current.push_back(parent);
                    }
                    for other_content in self.line_content(file_id, parent)? {
                        graph.add_edge((parent, other_content), (line_id, content), ());
                    }
                }
                for child in self.line_child(file_id, line_id)? {
                    if !visited.contains(&child) && !current.contains(&child) {
                        current.push_back(child);
                    }
                    for other_content in self.line_content(file_id, child)? {
                        graph.add_edge((line_id, content), (child, other_content), ());
                    }
                }
            }
        }

        // Make the graph acyclic
        let graph = condensation(graph.into_graph::<DefaultIx>(), true);

        // Make the graph linear
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

        // Flatten the conflict
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
        Ok(file)
    }

    fn file_diff(&self, file_id: FileId, target: &File) -> Result<HashSet<Change>, Self::Error> {
        todo!()
    }
}
