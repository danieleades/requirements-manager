//! An in-memory tree structure for requirements
//!
//! The [`Tree`] knows nothing about the filesystem or the directory structure.
//! It is a simple in-memory representation of the requirements and their relationships.

use std::{cmp::Ordering, collections::HashMap};
use tracing::instrument;
use uuid::Uuid;

use crate::{Requirement, domain::Hrid};

/// An in-memory representation of the set of requirements
#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    /// The requirements, stored contiguously.
    requirements: Vec<Requirement>,

    /// An index from UUID to position in `requirements`.
    index: HashMap<Uuid, usize>,

    /// A map from requirement kind to the next available index for that kind.
    next_indices: HashMap<String, usize>,
}

impl Tree {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            requirements: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
            next_indices: HashMap::new(),
        }
    }

    /// Inserts a requirement into the tree.
    /// Panics if a requirement with the same UUID already exists.
    pub fn insert(&mut self, requirement: Requirement) {
        let uuid = requirement.uuid();
        assert!(
            !self.index.contains_key(&uuid),
            "Duplicate requirement UUID: {uuid}"
        );
        let index = self.requirements.len();

        // Update the current index for the requirement's kind to the larger of its current value or the index of the incoming requirement.
        let Hrid { kind, id: suffix } = requirement.hrid();

        self.next_indices
            .entry(kind.to_string())
            .and_modify(|i| *i = (*i).max(suffix + 1))
            .or_insert(suffix + 1);

        self.requirements.push(requirement);
        self.index.insert(uuid, index);
    }

    /// Retrieves a requirement by UUID.
    pub fn requirement(&self, uuid: Uuid) -> Option<&Requirement> {
        self.index
            .get(&uuid)
            .and_then(|&idx| self.requirements.get(idx))
    }

    /// Read all the requirements and update any incorrect parent HRIDs.
    /// Returns an iterator of UUIDs whose parents were updated.
    #[instrument(skip(self))]
    pub fn update_hrids(&mut self) -> impl Iterator<Item = Uuid> + '_ {
        (0..self.requirements.len()).filter_map(|i| {
            let (left, right) = self.requirements.split_at_mut(i);
            let (req, right) = right.split_first_mut()?;
            let uuid = req.uuid();

            let updated: Vec<bool> = req
                .parents_mut()
                .map(|(parent_id, parent)| {
                    let &parent_idx = self
                        .index
                        .get(&parent_id)
                        .unwrap_or_else(|| panic!("Parent requirement {parent_id} not found!"));

                    let actual_hrid = match parent_idx.cmp(&i) {
                        Ordering::Less => left[parent_idx].hrid(),
                        Ordering::Greater => right[parent_idx - i - 1].hrid(),
                        Ordering::Equal => {
                            unreachable!("Requirement {parent_id} is its own parent!")
                        }
                    };

                    if parent.hrid == *actual_hrid {
                        false
                    } else {
                        parent.hrid = actual_hrid.clone();
                        true
                    }
                })
                // Collect here to ensure all parents are updated (no short-circuiting).
                .collect();

            // If any parent was updated, return the UUID of the requirement.
            updated
                .iter()
                .any(|was_updated| *was_updated)
                .then_some(uuid)
        })
    }

    /// Returns the next available index for a requirement of the given kind.
    ///
    /// This is one greater than the highest index currently used for that kind.
    /// No attempt is made to 'recycle' indices if there are gaps in the sequence.
    pub fn next_index(&self, kind: &str) -> usize {
        self.next_indices.get(kind).copied().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::Requirement;
    use crate::domain::Hrid;
    use crate::storage::Tree;

    fn make_requirement(uuid: Uuid, hrid: Hrid, parents: Vec<(Uuid, Hrid)>) -> Requirement {
        let mut req = Requirement::new_with_uuid(hrid, String::new(), uuid);
        for (parent_uuid, parent_hrid) in parents {
            req.add_parent(
                parent_uuid,
                crate::domain::requirement::Parent {
                    hrid: parent_hrid,
                    fingerprint: String::new(),
                },
            );
        }
        req
    }

    #[test]
    fn test_insert_and_lookup() {
        let mut tree = Tree::default();
        let uuid = Uuid::new_v4();
        let hrid = Hrid::try_from("R-001").unwrap();
        let req = make_requirement(uuid, hrid.clone(), vec![]);
        tree.insert(req);

        let retrieved = tree.requirement(uuid).unwrap();
        assert_eq!(retrieved.uuid(), uuid);
        assert_eq!(retrieved.hrid(), &hrid);
    }

    #[test]
    #[should_panic(expected = "Duplicate requirement UUID")]
    fn test_insert_duplicate_uuid_panics() {
        let mut tree = Tree::default();
        let uuid = Uuid::new_v4();
        let req1 = make_requirement(uuid, Hrid::try_from("R-001").unwrap(), vec![]);
        let req2 = make_requirement(uuid, Hrid::try_from("R-002").unwrap(), vec![]);
        tree.insert(req1);
        tree.insert(req2); // should panic
    }

    #[test]
    fn test_update_hrids_corrects_parent_hrids() {
        let mut tree = Tree::default();
        let parent_uuid = Uuid::new_v4();
        let child_uuid = Uuid::new_v4();

        let parent = make_requirement(parent_uuid, Hrid::try_from("P-001").unwrap(), vec![]);
        let child = make_requirement(
            child_uuid,
            Hrid::try_from("C-001").unwrap(),
            vec![(parent_uuid, Hrid::try_from("WRONG-001").unwrap())],
        );

        tree.insert(parent);
        tree.insert(child);

        let updated: Vec<_> = tree.update_hrids().collect();
        assert_eq!(updated, vec![child_uuid]);

        let updated_child = tree.requirement(child_uuid).unwrap();
        let (_, actual_parent) = updated_child.parents().next().unwrap();
        assert_eq!(actual_parent.hrid, Hrid::try_from("P-001").unwrap());
    }

    #[test]
    fn test_update_hrids_no_change() {
        let mut tree = Tree::default();
        let parent_uuid = Uuid::new_v4();
        let child_uuid = Uuid::new_v4();

        let parent = make_requirement(parent_uuid, Hrid::try_from("P-001").unwrap(), vec![]);
        let child = make_requirement(
            child_uuid,
            Hrid::try_from("C-001").unwrap(),
            vec![(parent_uuid, Hrid::try_from("P-001").unwrap())],
        );

        tree.insert(parent);
        tree.insert(child);

        let updated = tree.update_hrids();
        assert!(updated.count() == 0);
    }

    #[test]
    #[should_panic(expected = "Parent requirement")]
    fn test_update_hrids_missing_parent_panics() {
        let mut tree = Tree::default();
        let missing_uuid = Uuid::new_v4();
        let child_uuid = Uuid::new_v4();
        let child = make_requirement(
            child_uuid,
            Hrid::try_from("C-001").unwrap(),
            vec![(missing_uuid, Hrid::try_from("UNKNOWN-001").unwrap())],
        );

        tree.insert(child);
        let _ = tree.update_hrids().collect::<Vec<_>>();
    }

    #[test]
    #[should_panic(expected = "is its own parent")]
    fn test_update_hrids_self_parent_panics() {
        let mut tree = Tree::default();
        let uuid = Uuid::new_v4();
        let req = make_requirement(
            uuid,
            Hrid::try_from("SELF-001").unwrap(),
            vec![(uuid, Hrid::try_from("SELF-001").unwrap())],
        );

        tree.insert(req);
        let _ = tree.update_hrids().collect::<Vec<_>>();
    }
}
