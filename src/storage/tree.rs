use std::collections::HashMap;

use tracing::instrument;
use uuid::Uuid;

use crate::Requirement;

/// An in-memory representation of the set of requirements
#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    /// The requirements in the directory
    ///
    /// represented as a tuple of directory and the [`Requirement`].
    ///
    /// Note that the path doesn't contain the file stem, that's determined by the
    /// requirement's human-readable ID (HRID).
    ///
    /// TODO: address the fact the UUID is duplicated in both the key and the value in this hashmap.
    requirements: Vec<Requirement>,

    /// An index from UUID to a tuple of (requirement index, path to containing directory, HRID)
    index: HashMap<Uuid, (usize, String)>,
}

impl Tree {
    pub fn insert(&mut self, requirement: Requirement) {
        // TODO: handle collisions
        self.index.insert(
            requirement.uuid(),
            (self.requirements.len(), requirement.hrid().to_string()),
        );
        self.requirements.push(requirement);
    }

    pub fn requirement(&self, uuid: Uuid) -> Option<&Requirement> {
        self.index
            .get(&uuid)
            .and_then(|&(idx, _)| self.requirements.get(idx))
    }

    /// Read all the requirements files and update any incorrect parent HRIDs
    #[instrument(skip(self))]
    pub fn update_hrids(&mut self) -> impl Iterator<Item = Uuid> {
        self.requirements.iter_mut().filter_map(|req| {
            let id = req.uuid();
            tracing::trace!("checking requirement parent HRIDs: {id}");
            req.parents_mut().find_map(|(parent_id, parent)| {
                let actual_hrid = &self.index.get(&parent_id).unwrap().1;
                if &parent.hrid == actual_hrid {
                    None
                } else {
                    parent.hrid = actual_hrid.to_string();
                    Some(id)
                }
            })
        })
    }
}
