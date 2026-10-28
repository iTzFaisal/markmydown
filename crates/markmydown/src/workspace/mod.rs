//! Workspace tree, document state, file safety, session, and recovery.

pub mod atomic;
pub mod document;
pub mod encoding;
pub mod frontmatter;
pub mod import;
pub mod ops;
pub mod recovery;
pub mod session;
pub mod state;
pub mod tree;
pub mod watcher;
