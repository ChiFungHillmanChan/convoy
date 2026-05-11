//! Convoy daemon.

#![forbid(unsafe_code)]

pub mod rpc;
pub mod server;

pub use rpc::{Request, Response};
pub use server::Daemon;
