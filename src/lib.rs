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
//!   ──────────────────────────                ──────────────────────────
//!   parse bytes / decode                       cfg::Cfg          blocks + edges
//!   build expressions per block   ──────────► ir::BlockResult   stmts + term
//!   synthesize a VarTable                      structure::Structurer / sese
//!   machine idioms (javac / d8 / R8 / ...)  ──► convert::Converter (Region → Stmt)
//!   your refinement passes  ───────────────────► emit::Printer    Java source text
//!   (the shared pass suite is the last
//!    piece still living front-end-side)
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
pub mod emit;
pub mod ir;
pub mod rename;
pub mod sese;
pub mod structure;
pub mod types;
pub mod typeutil;
pub mod var;

pub use ctx::{ctx_method_body, Ctx, Family, MethodBody, NestedClass, NestedKind, NullCtx};
pub use ir::{BlockResult, Expr, Stmt, SwitchTargets, Term};
