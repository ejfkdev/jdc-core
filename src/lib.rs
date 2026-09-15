//! jdc-core — machine-neutral decompiler core.
//!
//! A decompiler is a pipeline: `machine code → CFG → per-block expressions →
//! control-flow structuring → statement tree → refinement passes → source`.
//! Only the first two steps know anything about the source machine (JVM `.class`
//! operand stack, DEX registers, ...). Everything after them operates on the
//! machine-neutral IR defined here, and that is what this crate provides.
//!
//! ```text
//!   front-end (machine-specific)                jdc-core (machine-neutral)
//!   ───────────────────────────                ──────────────────────────
//!   parse bytes                                 cfg::Cfg        (blocks + edges)
//!   decode instructions        ───────────────► ir::BlockResult (stmts + term)
//!   build expressions per block                 structure::Structurer / sese
//!   synthesize a VarTable                       convert::Converter
//!   machine idioms (javac / d8 / ...)  ────────► passes::* (shared refinements)
//!                                               emit::Printer   (source text)
//! ```
//!
//! The front-end implements [`ctx::Ctx`] (naming, type queries, nested method
//! bodies) and fills [`cfg::Cfg`] + [`ir::BlockResult`]. See `docs/CONTRACT.md`
//! for the exact obligations, and `tests/toy_frontend.rs` for a complete
//! non-JVM example.

pub mod analysis;
pub mod cfg;
pub mod convert;
pub mod ctx;
pub mod dbg;
pub mod var;
pub mod emit;
pub mod ir;
pub mod sese;
pub mod structure;
pub mod typeutil;
pub mod types;

pub use ctx::{Ctx, Family, MethodBody, NestedClass, NestedKind, NullCtx};
pub use ir::{BlockResult, Expr, Stmt, SwitchTargets, Term};