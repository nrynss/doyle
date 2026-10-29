//! Doyle's core: the citation index, flagging, proposals, decisions and the state store.
//!
//! The core knows only the adapter contract, never a particular memory system.

pub mod decide;

mod flag;
mod model;
mod store;

pub use flag::flag_citations;
pub use model::{
    ChangeId, Citation, Decision, DecisionVerdict, ElementId, Flag, Memory, MemoryRef, Proposal,
    ProposalAction, Severity,
};
