//! Machine-neutral statement/CFG predicates shared by the structuring and
//! emission layers.
//!
//! These answer questions about *Java control flow*, not about any bytecode:
//! "does this loop complete normally?", "does this statement leave a block?".
//! A front-end never needs to override them.

use crate::ir::expr::{ConstVal, Expr};
use crate::ir::stmt::Stmt;

/// True when a `while (true)` loop (optionally labeled) can NOT complete
/// normally, so the statements after it are unreachable and a decompiler may
/// truncate them.
///
/// JLS 14.14: `while (true)` completes normally iff its body holds a reachable
/// break that exits IT — an unlabeled break not captured by an inner breakable
/// construct, or `break L` where `L` is this loop's own label. A labeled break
/// to any other label (inner or outer) completes the loop abruptly.
pub fn dead_end_infinite_while(s: &Stmt) -> bool {
    let (label, body, cond) = match s {
        Stmt::While { cond, body } => (None, body.as_ref(), cond),
        Stmt::Labeled { label, body } => match &**body {
            Stmt::While { cond, body } => (Some(label.as_str()), body.as_ref(), cond),
            _ => return false,
        },
        _ => return false,
    };
    if !matches!(cond, Expr::Const(ConstVal::Int(1))) {
        return false;
    }
    !has_break_exiting(body, label, 0)
}

/// True when `s` contains a break that exits the loop labeled `lbl`
/// (`None` = the innermost unlabeled loop).
fn has_break_exiting(s: &Stmt, lbl: Option<&str>, depth: usize) -> bool {
    match s {
        Stmt::Break(None) => depth == 0,
        // Only this loop's OWN label completes it; any other labeled break
        // exits abruptly (JLS 14.14/14.17).
        Stmt::Break(Some(l)) => Some(l.as_str()) == lbl,
        Stmt::Continue(_) | Stmt::Return(_) | Stmt::Throw(_) => false,
        Stmt::Goto(_) => true,
        Stmt::Block(v) => v.iter().any(|x| has_break_exiting(x, lbl, depth)),
        Stmt::If {
            then_stmt,
            else_stmt,
            ..
        } => {
            has_break_exiting(then_stmt, lbl, depth)
                || else_stmt
                    .as_deref()
                    .map(|e| has_break_exiting(e, lbl, depth))
                    .unwrap_or(false)
        }
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::For { body, .. }
        | Stmt::ForEach { body, .. } => has_break_exiting(body, lbl, depth + 1),
        Stmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|x| has_break_exiting(x, lbl, depth + 1)))
                || default
                    .as_deref()
                    .map(|d| has_break_exiting(d, lbl, depth + 1))
                    .unwrap_or(false)
        }
        Stmt::Try {
            body,
            catches,
            finally,
        }
        | Stmt::TryWithResources {
            body,
            catches,
            finally,
            ..
        } => {
            has_break_exiting(body, lbl, depth)
                || catches
                    .iter()
                    .any(|c| has_break_exiting(&c.body, lbl, depth))
                || finally
                    .as_deref()
                    .map(|f| has_break_exiting(f, lbl, depth))
                    .unwrap_or(false)
        }
        Stmt::Synchronized { body, .. } => has_break_exiting(body, lbl, depth),
        Stmt::Labeled { body, .. } => has_break_exiting(body, lbl, depth),
        // Plain leaf statements never contain a break.
        Stmt::ExprStmt(_)
        | Stmt::LocalDef { .. }
        | Stmt::Assert { .. }
        | Stmt::TernaryValue { .. }
        | Stmt::MonitorEnter(_)
        | Stmt::MonitorExit(_)
        | Stmt::Comment(_) => false,
        // Opaque shapes (raw labels, inline class decls, anything new): keep
        // truncation off when an escape cannot be ruled out.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::build::int_const;

    fn while_true(body: Stmt) -> Stmt {
        Stmt::While {
            cond: int_const(1),
            body: Box::new(body),
        }
    }

    #[test]
    fn infinite_while_without_break_is_a_dead_end() {
        assert!(dead_end_infinite_while(&while_true(Stmt::Block(vec![]))));
        // A break inside a NESTED loop does not exit the outer while(true).
        let nested = Stmt::Block(vec![while_true(Stmt::Block(vec![Stmt::Break(None)]))]);
        assert!(dead_end_infinite_while(&while_true(nested)));
        // A direct break makes the loop completable.
        assert!(!dead_end_infinite_while(&while_true(Stmt::Block(vec![
            Stmt::Break(None)
        ]))));
        // A labeled break to an outer label exits abruptly.
        assert!(dead_end_infinite_while(&Stmt::Labeled {
            label: "L".into(),
            body: Box::new(while_true(Stmt::Block(vec![Stmt::Break(Some(
                "outer".into()
            ))]))),
        }));
    }
}
/// The renamed `$assertionsDisabled` field (javac reserves the original name).
pub const ASSERT_FIELD: &str = "$jcdcAssertionsDisabled";
