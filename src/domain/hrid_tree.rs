use std::collections::HashMap;

use non_empty_string::NonEmptyString;
use uuid::Uuid;

use crate::{
    domain::{self, tree::Tree, Fingerprint},
    Hrid, Requirement,
};

#[derive(Debug, Default)]
pub struct HridTree {
    tree: Tree,
    uuids: HashMap<Hrid, Uuid>,
    hrids: HashMap<Uuid, Hrid>,
    current_indices: HashMap<NonEmptyString, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    #[error("HRID {0} already exists and maps to a different UUID")]
    HridConflict(Hrid),
}

impl HridTree {
    /// Insert a requirement with an explicit HRID and UUID.
    ///
    /// Returns the old requirement if the UUID was already present.
    /// Fails if the HRID already maps to a different UUID.
    pub fn insert(
        &mut self,
        hrid: Hrid,
        uuid: Uuid,
        requirement: Requirement,
    ) -> Result<Option<Requirement>, InsertError> {
        if let Some(existing_uuid) = self.uuids.get(&hrid) {
            if existing_uuid != &uuid {
                return Err(InsertError::HridConflict(hrid));
            }
        }

        let current = self.current_indices.entry(hrid.kind.clone()).or_default();
        *current = (*current).max(hrid.id);

        self.uuids.insert(hrid.clone(), uuid);
        self.hrids.insert(uuid, hrid);

        Ok(self.tree.insert(uuid, requirement))
    }

    /// Generate a new UUID and HRID and insert the requirement. Returns the
    /// UUID and HRID.
    pub fn add(&mut self, kind: NonEmptyString, requirement: Requirement) -> (Uuid, &Hrid) {
        let uuid = Uuid::new_v4();
        let next = self.current_indices.entry(kind.clone()).or_default();
        *next += 1;

        let hrid = Hrid::new(kind, *next);

        self.uuids.insert(hrid.clone(), uuid);
        self.hrids.insert(uuid, hrid);
        self.tree.insert(uuid, requirement);

        (uuid, &self.hrids[&uuid])
    }

    pub fn get(&self, uuid: &Uuid) -> Option<(&Hrid, &Requirement)> {
        match (self.hrids.get(uuid), self.tree.get(uuid)) {
            (None, None) => None,
            (None, Some(_)) | (Some(_), None) => unreachable!(),
            (Some(hrid), Some(requirement)) => Some((hrid, requirement)),
        }
    }

    /// Get a requirement by HRID.
    ///
    /// Returns the associated UUID and the requirement, if it exists.
    pub fn get_by_hrid(&self, hrid: &Hrid) -> Option<(&Uuid, &Requirement)> {
        self.uuids.get(hrid).map(|uuid| {
            self.tree.get(uuid).map_or_else(
                || unreachable!("HRID maps to UUID, but requirement is missing"),
                |requirement| (uuid, requirement),
            )
        })
    }

    pub fn link(&mut self, child: Uuid, parent: Uuid) -> Result<(), domain::tree::LinkError> {
        self.tree.link(child, parent)
    }

    pub fn link_by_hrid(&mut self, child: &Hrid, parent: &Hrid) -> Result<(), LinkError> {
        match (self.uuids.get(child), self.uuids.get(parent)) {
            (None, None) | (None, Some(_)) => Err(LinkError::NotFound(child.clone())),
            (Some(_), None) => Err(LinkError::NotFound(parent.clone())),
            (Some(child_uuid), Some(parent_uuid)) => self
                .tree
                .link(*child_uuid, *parent_uuid)
                .map_err(|e| match e {
                    // This must be unreachable, since we found the UUIDs
                    domain::tree::LinkError::RequirementNotFound(..) => unreachable!(),
                    domain::tree::LinkError::SelfReference(..) => {
                        LinkError::SelfReference(child.clone())
                    }
                    domain::tree::LinkError::WouldCreateCycle { .. } => {
                        LinkError::WouldCreateCycle {
                            child: child.clone(),
                            parent: parent.clone(),
                        }
                    }
                }),
        }
    }

    pub fn hrid(&self, uuid: &Uuid) -> Option<&Hrid> {
        self.hrids.get(uuid)
    }

    pub fn parents(
        &self,
        uuid: Uuid,
    ) -> impl std::iter::Iterator<Item = (uuid::Uuid, &Fingerprint)> + '_ {
        self.tree.parents(uuid)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("Requirement {0} not found")]
    NotFound(Hrid),

    #[error("Self-reference not allowed: {0}")]
    SelfReference(Hrid),

    #[error("Linking {child} to {parent} would create a cycle")]
    WouldCreateCycle { child: Hrid, parent: Hrid },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use non_empty_string::NonEmptyString;
    use uuid::Uuid;

    use crate::{
        domain::{hrid_tree::HridTree, Requirement},
        Hrid,
    };

    fn hrid(prefix: &str, id: usize) -> Hrid {
        Hrid::new(NonEmptyString::from_str(prefix).unwrap(), id)
    }

    #[test]
    fn add_and_get() {
        let mut tree = HridTree::default();
        let (uuid, hrid) = tree.add(
            "REQ".parse().unwrap(),
            Requirement::new("requirement text".into()),
        );
        let hrid = hrid.clone();

        let (fetched_hrid, req) = tree.get(&uuid).unwrap();
        assert_eq!(fetched_hrid, &hrid);
        assert_eq!(req.content(), "requirement text");

        let (fetched_uuid, req2) = tree.get_by_hrid(&hrid).unwrap();
        assert_eq!(fetched_uuid, &uuid);
        assert_eq!(req2.content(), "requirement text");
    }

    #[test]
    #[should_panic(expected = "HRID maps to UUID, but requirement is missing")]
    fn get_by_hrid_inconsistent_should_panic() {
        let mut tree = HridTree::default();
        let uuid = Uuid::new_v4();
        let hrid = hrid("REQ", 1);
        tree.uuids.insert(hrid.clone(), uuid);
        tree.get_by_hrid(&hrid); // panics
    }

    #[test]
    fn get_missing_returns_none() {
        let tree = HridTree::default();
        let hrid = hrid("REQ", 999);
        let uuid = Uuid::new_v4();

        assert!(tree.get(&uuid).is_none());
        assert!(tree.get_by_hrid(&hrid).is_none());
    }
}
