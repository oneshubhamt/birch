//! Application workflows coordinating domain/storage capabilities for UI surfaces.
//!
//! This layer is intentionally concrete and small. UI code owns presentation state and
//! editor/browser transport; application services own multi-step Rust workflows.

pub(crate) mod attachments;
pub(crate) mod context;
pub(crate) mod editor;
pub(crate) mod editor_workflows;
pub(crate) mod errors;
pub(crate) mod executor;
pub(crate) mod export;
pub(crate) mod library;
pub(crate) mod notes;
pub(crate) mod persistence;
pub(crate) mod portability;
pub(crate) mod privacy;
pub(crate) mod runtime;
pub(crate) mod settings;
pub(crate) mod vault;
