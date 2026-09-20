//! The front-end → core hand-off: how a basic block ends and what it produced.
//!
//! A front-end walks its own instruction stream, splits it into blocks that
//! match [`crate::cfg::Cfg`], and emits one [`BlockResult`] per block, indexed
//! by block id. Everything downstream (structuring, conversion, emission)
//! works only from these values.

use crate::ir::expr::{BinOp, ConcatPart, ConstVal, Expr};
use crate::ir::stmt::Stmt;
use crate::types::JavaType;

/// How a basic block ends.
#[derive(Debug, Clone)]
pub enum Term {
    Fallthrough,
    Goto,
    /// Jumps to `succ[1]` when `cond` is true; falls through to `succ[0]`.
    Cond {
        cond: Expr,
    },
    /// A multi-way branch. `default` is the machine offset of the default
    /// arm when the front-end can identify it (JVM: the switch payload's
    /// default; DEX: the `packed-switch`/`sparse-switch` payload's default);
    /// `None` means "unknown / not recorded".
    Switch {
        selector: Expr,
        targets: SwitchTargets,
        default: Option<u32>,
    },
    Return(Option<Expr>),
    Throw(Expr),
    /// Legacy `jsr` (JVM only; kept so legacy class files can be modelled).
    Jsr,
    /// Legacy `ret` (JVM only).
    Ret,
}

#[derive(Debug, Clone)]
pub enum SwitchTargets {
    /// `targets[i]` is the target for key `low + i`.
    Table { low: i32, targets: Vec<u32> },
    /// `(match value, target)` pairs.
    Lookup { pairs: Vec<(i32, u32)> },
}

#[derive(Debug, Clone)]
pub struct BlockResult {
    /// Statements produced by this block, in order, covering everything up to
    /// (not including) the terminator.
    pub stmts: Vec<Stmt>,
    /// Value-machine leftovers at block exit — for a stack machine, the
    /// operand stack; for a register machine, normally empty. Only the
    /// front-end's own merge handling consumes this.
    pub out_stack: Vec<Expr>,
    pub term: Term,
}

#[derive(Debug)]
pub struct BuildError(pub String);

pub type BResult<T> = Result<T, BuildError>;

/// `Expr::Const(ConstVal::Int(i))`.
pub fn int_const(i: i32) -> Expr {
    Expr::Const(ConstVal::Int(i))
}

/// True if dropping this expression from a discarded-value context would
/// change semantics (a call, a store, a field read that can trap, ...).
pub fn has_side_effects(e: &Expr) -> bool {
    match e {
        Expr::Const(_) | Expr::Local { .. } | Expr::This | Expr::Raw(_) | Expr::RawT(..) => false,
        Expr::New { .. }
        | Expr::Method { .. }
        | Expr::Invokedynamic { .. }
        | Expr::Lambda(_)
        | Expr::AnonNew { .. } => true,
        Expr::Assign { .. } | Expr::PreIncDec { .. } | Expr::PostIncDec { .. } => true,
        Expr::Field { owner, .. } => {
            // getfield can NPE / trigger clinit; keep to be safe unless owner is this
            owner.is_some()
        }
        Expr::ArrayIndex { .. } => true,
        Expr::Un { e, .. } | Expr::Cast { e, .. } | Expr::InstanceOf { e, .. } => {
            has_side_effects(e)
        }
        Expr::Bin { l, r, .. } => has_side_effects(l) || has_side_effects(r),
        Expr::Cond { c, t, f } => has_side_effects(c) || has_side_effects(t) || has_side_effects(f),
        Expr::NewArray { dims, init, .. } => {
            dims.iter().any(has_side_effects)
                || init
                    .as_ref()
                    .map(|v| v.iter().any(has_side_effects))
                    .unwrap_or(false)
        }
        Expr::NewMultiArray { dims, .. } => dims.iter().any(has_side_effects),
        Expr::StringConcat(parts) => parts.iter().any(|p| match p {
            ConcatPart::Str(e) => has_side_effects(e),
            ConcatPart::Const(_) => false,
        }),
    }
}

/// If the value is a cmp sentinel, unfold into `(l, r, op)`; otherwise compare
/// against zero.
pub fn unfold_cmp(v: Expr, op_if_int: BinOp) -> (Expr, Expr, BinOp) {
    if let Expr::Invokedynamic { name, args, .. } = &v {
        if name.starts_with("\u{0}cmp") && args.len() == 2 {
            let mut args = match v {
                Expr::Invokedynamic { args, .. } => args,
                _ => unreachable!(),
            };
            let r = args.pop().unwrap();
            let l = args.pop().unwrap();
            return (l, r, op_if_int);
        }
    }
    (v, int_const(0), op_if_int)
}

/// A machine type reference (`[`-prefixed descriptors are array types).
pub fn class_name_to_type(name: &str) -> JavaType {
    if name.starts_with('[') {
        crate::types::parse_field_descriptor(name).unwrap_or(JavaType::Object(name.into()))
    } else {
        JavaType::Object(name.into())
    }
}
