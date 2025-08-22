use std::collections::{HashMap, HashSet};

use petgraph::{prelude::DiGraphMap, Direction};
use uuid::Uuid;

use crate::{domain::Fingerprint, Requirement};

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("Requirement {0} not found")]
    RequirementNotFound(Uuid),

    #[error("Self-reference not allowed: {0}")]
    SelfReference(Uuid),

    #[error("Linking {child} to {parent} would create a cycle")]
    WouldCreateCycle { child: Uuid, parent: Uuid },
}

// --- Recursive Traversal Iterator ---

pub struct Recursive<'a> {
    tree: &'a Tree,
    stack: Vec<Uuid>,
    visited: HashSet<Uuid>,
    direction: Direction,
}

impl Iterator for Recursive<'_> {
    type Item = Uuid;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            // Expand first to avoid dropping siblings
            for n in self.tree.graph.neighbors_directed(node, self.direction) {
                if self.visited.insert(n) {
                    self.stack.push(n);
                }
            }
            return Some(node); // always yield the popped node
        }
        None
    }
}

// --- Main Tree Structure ---

#[derive(Debug, Clone, Default)]
pub struct Tree {
    graph: DiGraphMap<Uuid, Fingerprint>,
    requirements: HashMap<Uuid, Requirement>,
}

impl Tree {
    pub fn insert(&mut self, uuid: Uuid, requirement: Requirement) -> Option<Requirement> {
        let old = self.requirements.insert(uuid, requirement);
        self.graph.add_node(uuid);
        old
    }

    pub fn get(&self, uuid: &Uuid) -> Option<&Requirement> {
        self.requirements.get(uuid)
    }

    pub fn get_mut(&mut self, uuid: &Uuid) -> Option<&mut Requirement> {
        self.requirements.get_mut(uuid)
    }

    pub fn remove(&mut self, uuid: &Uuid) -> Option<Requirement> {
        let req = self.requirements.remove(uuid)?;
        self.graph.remove_node(*uuid);
        Some(req)
    }

    pub fn link(&mut self, child: Uuid, parent: Uuid) -> Result<(), LinkError> {
        if child == parent {
            return Err(LinkError::SelfReference(child));
        }
        if !self.requirements.contains_key(&child) {
            return Err(LinkError::RequirementNotFound(child));
        }
        let Some(parent_req) = self.get(&parent) else {
            return Err(LinkError::RequirementNotFound(parent));
        };
        if self.would_create_cycle(child, parent) {
            return Err(LinkError::WouldCreateCycle { child, parent });
        }

        let fingerprint = parent_req.fingerprint();

        self.graph.add_edge(child, parent, fingerprint);
        Ok(())
    }

    pub fn unlink(&mut self, child: Uuid, parent: Uuid) -> Option<Fingerprint> {
        self.graph.remove_edge(child, parent)
    }

    pub fn parents(&self, uuid: Uuid) -> impl Iterator<Item = (Uuid, &Fingerprint)> + '_ {
        self.graph
            .edges_directed(uuid, Direction::Outgoing)
            .map(|(_child, parent, fp)| (parent, fp))
    }

    pub fn children(&self, uuid: Uuid) -> impl Iterator<Item = (Uuid, &Fingerprint)> + '_ {
        self.graph
            .edges_directed(uuid, Direction::Incoming)
            .map(|(child, _parent, fp)| (child, fp))
    }

    fn walk(&self, start: Uuid, direction: Direction) -> impl Iterator<Item = Uuid> + '_ {
        Recursive {
            tree: self,
            stack: vec![start],
            visited: HashSet::from([start]),
            direction,
        }
    }

    // Exclude the seed at the API boundary
    pub fn ancestors(&self, uuid: Uuid) -> impl Iterator<Item = Uuid> + '_ {
        self.walk(uuid, Direction::Outgoing).skip(1)
    }

    pub fn descendants(&self, uuid: Uuid) -> impl Iterator<Item = Uuid> + '_ {
        self.walk(uuid, Direction::Incoming).skip(1)
    }

    pub fn topological_order(&self) -> Result<Vec<Uuid>, Vec<Uuid>> {
        petgraph::algo::toposort(&self.graph, None).map_err(|e| vec![e.node_id()])
    }

    pub fn cycles(&self) -> Vec<Vec<Uuid>> {
        petgraph::algo::kosaraju_scc(&self.graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .collect()
    }

    fn would_create_cycle(&self, child: Uuid, parent: Uuid) -> bool {
        petgraph::algo::has_path_connecting(&self.graph, parent, child, None)
    }

    pub fn contains(&self, uuid: &Uuid) -> bool {
        self.requirements.contains_key(uuid)
    }

    pub fn len(&self) -> usize {
        self.requirements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty()
    }

    pub fn uuids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.requirements.keys().copied()
    }

    pub fn requirements(&self) -> impl Iterator<Item = (Uuid, &Requirement)> + '_ {
        self.requirements.iter().map(|(&id, req)| (id, req))
    }

    pub fn requirements_mut(&mut self) -> impl Iterator<Item = (Uuid, &mut Requirement)> + '_ {
        self.requirements.iter_mut().map(|(&id, req)| (id, req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut tree = Tree::default();
        let id = Uuid::new_v4();
        assert!(tree.insert(id, Requirement::new("Req A".into())).is_none());
        assert_eq!(tree.get(&id).unwrap().content(), "Req A");
        assert!(tree.remove(&id).is_some());
        assert!(tree.get(&id).is_none());
    }

    #[test]
    fn link_and_unlink() {
        let mut tree = Tree::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let requirement_a = Requirement::new("A".into());
        let fingerprint_a = requirement_a.fingerprint();
        tree.insert(a, requirement_a);
        tree.insert(b, Requirement::new("B".into()));

        assert!(tree.link(b, a).is_ok());
        let parents: Vec<_> = tree.parents(b).collect();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].0, a);

        let removed = tree.unlink(b, a).unwrap();
        assert_eq!(removed, fingerprint_a);
        assert_eq!(tree.parents(b).count(), 0);
    }

    #[test]
    fn detect_cycle() {
        let mut tree = Tree::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        tree.insert(a, Requirement::new("A".into()));
        tree.insert(b, Requirement::new("B".into()));

        assert!(tree.link(b, a).is_ok());
        let err = tree.link(a, b).unwrap_err();
        matches!(err, LinkError::WouldCreateCycle { .. });
    }

    #[test]
    fn topo_sort_order() {
        let mut tree = Tree::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        tree.insert(a, Requirement::new("A".into()));
        tree.insert(b, Requirement::new("B".into()));
        tree.insert(c, Requirement::new("C".into()));

        tree.link(b, a).unwrap();
        tree.link(c, b).unwrap();

        let order = tree.topological_order().unwrap();
        let pos = |x| order.iter().position(|&id| id == x).unwrap();
        assert!(pos(c) < pos(b) && pos(b) < pos(a));
    }

    #[test]
    fn ancestors_and_descendants() {
        let mut tree = Tree::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        tree.insert(a, Requirement::new("A".into()));
        tree.insert(b, Requirement::new("B".into()));
        tree.insert(c, Requirement::new("C".into()));

        tree.link(b, a).unwrap();
        tree.link(c, b).unwrap();

        let ancestors: HashSet<_> = tree.ancestors(c).collect();
        let descendants: HashSet<_> = tree.descendants(a).collect();

        assert!(ancestors.contains(&b));
        assert!(ancestors.contains(&a));
        assert!(descendants.contains(&b));
        assert!(descendants.contains(&c));
    }

    #[test]
    fn ancestors_descendants_with_branching() {
        let mut tree = Tree::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();

        // a <- b, a <- c, b <- d  (edges: child -> parent)
        for (id, name) in [(a, "A"), (b, "B"), (c, "C"), (d, "D")] {
            tree.insert(id, Requirement::new(name.into()));
        }
        tree.link(b, a).unwrap();
        tree.link(c, a).unwrap();
        tree.link(d, b).unwrap();

        let ancs_of_d: HashSet<_> = tree.ancestors(d).collect();
        assert!(ancs_of_d.contains(&b));
        assert!(ancs_of_d.contains(&a));
        assert!(!ancs_of_d.contains(&d));
        // From a, descendants must include both branches b, c and downstream d
        let desc_of_a: HashSet<_> = tree.descendants(a).collect();
        for id in [b, c, d] {
            assert!(desc_of_a.contains(&id));
        }
    }
}
