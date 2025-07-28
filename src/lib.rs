//! Plain-text Requirements Management
//!
//! Requirements are markdown documents stored in a directory.

mod domain;
pub use domain::{EmptyStringError, Hrid, Requirement};

mod storage;
pub use storage::Directory;
