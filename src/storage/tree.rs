//! An in-memory tree structure for requirements
//!
//! The [`Tree`] knows nothing about the filesystem or the directory structure.
//! It is a simple in-memory representation of the requirements and their relationships.

use std::{cmp::Ordering, collections::HashMap};
use tracing::instrument;
use uuid::Uuid;

use crate::Requirement;

/// An in-memory representation of the set of requirements
#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    /// The requirements, stored contiguously.
    requirements: Vec<Requirement>,

    /// An index from UUID to position in `requirements`.
    index: HashMap<Uuid, usize>,

    /// for HRID → UUID resolution, non-authoritative
    hrid_index: HashMap<String, Uuid>,
}

impl Tree {
    /// Inserts a requirement into the tree.
    /// Panics if a requirement with the same UUID already exists.
    pub fn insert(&mut self, requirement: Requirement) {
        let uuid = requirement.uuid();
        assert!(
            !self.index.contains_key(&uuid),
            "Duplicate requirement UUID: {uuid}"
        );
        let index = self.requirements.len();
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

                    if parent.hrid == actual_hrid {
                        false
                    } else {
                        parent.hrid = actual_hrid.to_string();
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
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::Requirement;
    use crate::storage::Tree;

    fn make_requirement(uuid: Uuid, hrid: &str, parents: Vec<(Uuid, String)>) -> Requirement {
        let mut req = Requirement::new_with_uuid(hrid.to_string(), String::new(), uuid);
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
        let req = make_requirement(uuid, "R-001", vec![]);
        tree.insert(req);

        let retrieved = tree.requirement(uuid).unwrap();
        assert_eq!(retrieved.uuid(), uuid);
        assert_eq!(retrieved.hrid(), "R-001");
    }

    #[test]
    #[should_panic(expected = "Duplicate requirement UUID")]
    fn test_insert_duplicate_uuid_panics() {
        let mut tree = Tree::default();
        let uuid = Uuid::new_v4();
        let req1 = make_requirement(uuid, "R-001", vec![]);
        let req2 = make_requirement(uuid, "R-002", vec![]);
        tree.insert(req1);
        tree.insert(req2); // should panic
    }

    #[test]
    fn test_update_hrids_corrects_parent_hrids() {
        let mut tree = Tree::default();
        let parent_uuid = Uuid::new_v4();
        let child_uuid = Uuid::new_v4();

        let parent = make_requirement(parent_uuid, "P-001", vec![]);
        let child = make_requirement(
            child_uuid,
            "C-001",
            vec![(parent_uuid, "WRONG".to_string())],
        );

        tree.insert(parent);
        tree.insert(child);

        let updated: Vec<_> = tree.update_hrids().collect();
        assert_eq!(updated, vec![child_uuid]);

        let updated_child = tree.requirement(child_uuid).unwrap();
        let (_, actual_parent) = updated_child.parents().next().unwrap();
        assert_eq!(actual_parent.hrid, "P-001");
    }

    #[test]
    fn test_update_hrids_no_change() {
        let mut tree = Tree::default();
        let parent_uuid = Uuid::new_v4();
        let child_uuid = Uuid::new_v4();

        let parent = make_requirement(parent_uuid, "P-001", vec![]);
        let child = make_requirement(
            child_uuid,
            "C-001",
            vec![(parent_uuid, "P-001".to_string())],
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
            "C-001",
            vec![(missing_uuid, "UNKNOWN".to_string())],
        );

        tree.insert(child);
        let _ = tree.update_hrids().collect::<Vec<_>>();
    }

    #[test]
    #[should_panic(expected = "is its own parent")]
    fn test_update_hrids_self_parent_panics() {
        let mut tree = Tree::default();
        let uuid = Uuid::new_v4();
        let req = make_requirement(uuid, "SELF", vec![(uuid, "SELF".to_string())]);

        tree.insert(req);
        let _ = tree.update_hrids().collect::<Vec<_>>();
    }
}
