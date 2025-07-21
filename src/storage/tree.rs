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
        let len = self.requirements.len();

        (0..len).filter_map(move |i| {
            let (left, right) = self.requirements.split_at_mut(i);
            let (req, right) = right.split_first_mut()?; // req = self.requirements[i]

            let updated = req.parents_mut().any(|(parent_id, parent)| {
                let &parent_idx = self
                    .index
                    .get(&parent_id)
                    .unwrap_or_else(|| panic!("Parent requirement {parent_id} not found!"));

                // Safe: parent_idx != i is enforced by match arms
                let actual_hrid = match parent_idx.cmp(&i) {
                    Ordering::Less => left[parent_idx].hrid(),
                    Ordering::Greater => right[parent_idx - i - 1].hrid(),
                    Ordering::Equal => panic!("Requirement {parent_id} is its own parent!"),
                };

                if parent.hrid == actual_hrid {
                    false
                } else {
                    parent.hrid = actual_hrid.to_string();
                    true
                }
            });

            updated.then(|| req.uuid())
        })
    }
}
