//! mini-agi-core — THE KERNEL: enforcement-bound memory, evaluation,
//! skills registry, checkpoint journal (port of `PoC` behavioral contract,
//! tag v1-spec-reference).
//!
//! Library-only, zero I/O dependencies beyond the filesystem; the binary
//! crate (mini-agi) is a thin shell exposing CLI + MCP server.

pub mod audit;
pub mod capture;
pub mod config;
pub mod contract;
pub mod dream;
pub mod eval;
pub mod failure;
pub mod harness;
pub mod hash;
pub mod health;
pub mod insights;
pub mod journal;
pub mod loopcmd;
pub mod memory;
pub mod metrics;
pub mod mismatch;
pub mod redact;
pub mod skills;
pub mod store;
pub mod ticket;
pub mod verifier;
pub mod worker;
