//! Shared data model for QuarkCraftLauncher.
//!
//! Nothing in this crate performs IO: it is the vocabulary shared by the
//! manager, the wrapper, the IPC layer and the frontends.

pub mod error;
pub mod model;

pub use error::{Error, Result};
pub use model::*;
