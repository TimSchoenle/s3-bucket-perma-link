//! The two routes, in the order they have to be registered.
//!
//! [`download`] is a catch-all, so anything registered after it is unreachable. [`health_check`]
//! goes first for that reason, and that is also why no permanent link can be spelled `/health`.

pub mod download;
pub mod health_check;
