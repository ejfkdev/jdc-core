//! Machine-neutral IR: expressions, statements, and the per-block build result.
//!
//! These types are what a front-end produces and what everything in this crate
//! consumes. They describe *Java source*, not any particular bytecode: a
//! `Term::Cond` carries the condition expression, a `SwitchTargets::Table`
//! carries the decoded key range — the machine-specific decoding happened
//! before, in the front-end.

pub mod build;
pub mod expr;
pub mod stmt;

pub use build::{BlockResult, BuildError, SwitchTargets, Term};
pub use expr::{
    AssignOp, BinOp, ConcatPart, ConstVal, Expr, LambdaExpr, LambdaKind, TypeRef, UnOp,
};
pub use stmt::Stmt;