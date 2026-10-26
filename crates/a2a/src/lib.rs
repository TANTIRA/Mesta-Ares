//! A2A (Agent2Agent) protocol support for ARES.
//!
//! Protocol types ([`card`], [`task`]) plus a transport-agnostic JSON-RPC
//! dispatcher ([`rpc`]) covering `tasks/send`, `tasks/get`, `tasks/cancel`
//! and `tasks/sendSubscribe`. The embedding service implements
//! [`rpc::TaskBackend`] to wire tasks to its own storage and agent bus.

#![warn(missing_docs)]

pub mod card;
pub mod rpc;
pub mod task;
