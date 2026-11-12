//! One module per verb, each a thin resolution of flags against
//! configuration followed by one call into `tradecraft_core`.
//!
//! `marketplace` backs two verbs: `build` and `check` are one path in two
//! modes, exactly as they are in the core.

pub mod bump;
pub mod init;
pub mod marketplace;
