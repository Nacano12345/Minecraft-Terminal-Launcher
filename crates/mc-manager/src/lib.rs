//! Manager: the single source of truth.
//!
//! Owns version metadata, the downloader, Java discovery, authentication and
//! launch plan assembly. It performs no rendering and spawns no game process
//! (that is the wrapper's job).

pub mod auth;
pub mod download;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod manifest;
pub mod paths;
pub mod rules;
pub mod version;

mod manager;

pub use manager::{Manager, DEFAULT_MANIFEST};
