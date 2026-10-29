//! Doyle's core: the citation index, flagging, proposals, decisions and the state store.
//!
//! The core knows only the adapter contract, never a particular memory system.

mod flag;
mod model;
mod store;

// Nothing uses the re-export until T2.3 wires up `doyle check`;
// the allow keeps the gate quiet until then.
#[allow(unused_imports)]
pub use flag::flag_citations;
// Nothing uses the re-export until T1.5 turns the crate into a library;
// the allow keeps the gate quiet until then.
#[allow(unused_imports)]
pub use model::{
    ChangeId, Citation, Decision, DecisionVerdict, ElementId, Flag, Memory, MemoryRef, Proposal,
    ProposalAction, Severity,
};
