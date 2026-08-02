//! mini-agi-core — THE KERNEL: enforcement-bound memory, evaluation,
//! skills registry, checkpoint journal (port of `PoC` behavioral contract,
//! tag v1-spec-reference).
//!
//! Library-only, zero I/O dependencies beyond the filesystem; the binary
//! crate (mini-agi) is a thin shell exposing CLI + MCP server.

pub mod hash;
pub mod memory;
pub mod store;
