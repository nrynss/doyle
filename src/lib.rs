//! Doyle: truth maintenance for agent memory.
//!
//! Library crate: every module is declared here so integration and live
//! tests under `tests/` can reach the code. The binary in `src/main.rs`
//! is a thin dispatcher over [`cli`].

pub mod adapters;
pub mod check;
pub mod cite;
pub mod cli;
pub mod contract;
pub mod core;
pub mod curate;
pub mod diffrun_cmd;
pub mod evidence;
pub mod guard;
pub mod mcp;
pub mod models;
pub mod prompts;
pub mod serve;
