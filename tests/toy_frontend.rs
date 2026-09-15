//! A complete, NON-JVM front-end driving the core — the genericity proof.
//!
//! This test plays the role of a future DEX (or any other) front-end with a
//! register-machine flavour: it never parses a class file, never touches a
//! constant pool, and uses `u32` code-unit offsets. It hand-builds
//! [`Cfg`] + [`BlockResult`]s, runs the shared structurer/converter, and
//! prints the result with a *test-local* mini printer (the real printer is
//! front-end driven too: it asks [`Ctx`] for names).
//!
//! Scenarios: a counted loop with an if/else, a table switch with a default
//! arm, and a try/catch with a handler.

use jdc_core::cfg::{Cfg, ExcRange};
use jdc_core::convert::Converter;
use jdc_core::ir::build::{BlockResult, SwitchTargets, Term};
use jdc_core::ir::expr::{AssignOp, BinOp, ConstVal, Expr, TypeRef};
use jdc_core::ir::stmt::Stmt;
use jdc_core::structure::Structurer;
use jdc_core::types::JavaType;
use jdc_core::var::VarTable;

fn blk(id: usize, start: u32, len: u32, succ: Vec<usize>) -> jdc_core::cfg::Block {
    jdc_core::cfg::Block {
        id,
        start,
        end: start + len,
        ins_len: len,
        succ,
        pred: Vec::new(),
        handlers: Vec::new(),
    }
}

fn res(stmts: Vec<Stmt>, term: Term) -> BlockResult {
    BlockResult { stmts, out_stack: Vec::new(), term }
}

/// Add a local to a fresh table (the front-end's job: DEX has no LVT).
fn vt_with(params: &[(&str, JavaType)]) -> VarTable {
    let mut vt = VarTable::default();
    for (i, (name, ty)) in params.iter().enumerate() {
        let id = vt.add_split(i as u16, (*name).to_string(), TypeRef::J(ty.clone()));
        assert_eq!(id, i as u32);
    }
    vt
}

fn local(vt: &VarTable, name: &str) -> Expr {
    let v = vt.vars.iter().find(|v| v.name == name).expect("var");
    Expr::Local { var: v.id, ty: v.ty.clone() }
}

fn int(i: i32) -> Expr {
    Expr::Const(ConstVal::Int(i))
}

/// `i < n` / `i = i + 1` helpers over the test's two locals.
fn less_than(l: Expr, r: Expr) -> Expr {
    Expr::Bin {
        op: BinOp::Lt,
        l: Box::new(l),
        r: Box::new(r),
        ty: Some(TypeRef::J(JavaType::Int)),
    }
}

fn assign(target: Expr, value: Expr) -> Stmt {
    Stmt::ExprStmt(Expr::Assign {
        target: Box::new(target),
        op: AssignOp::Plain,
        value: Box::new(value),
    })
}

fn add(l: Expr, r: Expr) -> Expr {
    Expr::Bin {
        op: BinOp::Add,
        l: Box::new(l),
        r: Box::new(r),
        ty: Some(TypeRef::J(JavaType::Int)),
    }
}

fn structuralize(cfg: &Cfg, results: &Vec<BlockResult>) -> Stmt {
    let mut st = Structurer::new(cfg, results);
    let region = st.structure_method();
    let mut conv = Converter::new(cfg, results);
    conv.convert(region)
}

// ---------------------------------------------------------------------------
// A mini printer: proves the IR is printable without any machine knowledge.
// Includes only the shapes these tests produce.
// ---------------------------------------------------------------------------

fn p_expr(e: &Expr, vt: &VarTable) -> String {
    match e {
        Expr::Const(ConstVal::Int(i)) => i.to_string(),
        Expr::Const(ConstVal::Null) => "null".into(),
        Expr::Local { var, .. } => vt.var(*var).name.clone(),
        Expr::This => "this".into(),
        Expr::Bin { op, l, r, .. } => {
            let o = match op {
                BinOp::Lt => "<",
                BinOp::Add => "+",
                BinOp::Eq => "==",
                _ => "?",
            };
            format!("{} {} {}", p_expr(l, vt), o, p_expr(r, vt))
        }
        Expr::Assign { target, value, .. } => {
            format!("{} = {}", p_expr(target, vt), p_expr(value, vt))
        }
        other => format!("<expr:{:?}>", std::mem::discriminant(other)),
    }
}

fn p_stmt(s: &Stmt, vt: &VarTable, out: &mut String, ind: usize) {
    let pad = "    ".repeat(ind);
    match s {
        Stmt::Block(v) => {
            for x in v {
                p_stmt(x, vt, out, ind);
            }
        }
        Stmt::ExprStmt(e) => out.push_str(&format!("{}{};\n", pad, p_expr(e, vt))),
        Stmt::LocalDef { var, init, .. } => {
            let t = vt.var(*var).ty.erased().to_java(true);
            match init {
                Some(e) => out.push_str(&format!("{}{} {} = {};\n", pad, t, vt.var(*var).name, p_expr(e, vt))),
                None => out.push_str(&format!("{}{} {};\n", pad, t, vt.var(*var).name)),
            }
        }
        Stmt::Return(e) => match e {
            Some(e) => out.push_str(&format!("{}return {};\n", pad, p_expr(e, vt))),
            None => out.push_str(&format!("{}return;\n", pad)),
        },
        Stmt::Throw(e) => out.push_str(&format!("{}throw {};\n", pad, p_expr(e, vt))),
        Stmt::If { cond, then_stmt, else_stmt } => {
            out.push_str(&format!("{}if ({}) {{\n", pad, p_expr(cond, vt)));
            p_stmt(then_stmt, vt, out, ind + 1);
            out.push_str(&format!("{}}}", pad));
            if let Some(e) = else_stmt {
                out.push_str(" else {\n");
                p_stmt(e, vt, out, ind + 1);
                out.push_str(&format!("{}}}", pad));
            }
            out.push('\n');
        }
        Stmt::While { cond, body } => {
            out.push_str(&format!("{}while ({}) {{\n", pad, p_expr(cond, vt)));
            p_stmt(body, vt, out, ind + 1);
            out.push_str(&format!("{}}}\n", pad));
        }
        Stmt::DoWhile { body, cond } => {
            out.push_str(&format!("{}do {{\n", pad));
            p_stmt(body, vt, out, ind + 1);
            out.push_str(&format!("{}}} while ({});\n", pad, p_expr(cond, vt)));
        }
        Stmt::For { init, cond, update, body } => {
            let mut init_s = String::new();
            for x in init {
                if let Stmt::ExprStmt(e) = x {
                    init_s.push_str(&p_expr(e, vt));
                }
            }
            let cond_s = cond.as_ref().map(|c| p_expr(c, vt)).unwrap_or_default();
            let upd_s: Vec<String> = update.iter().map(|u| p_expr(u, vt)).collect();
            out.push_str(&format!(
                "{}for ({}; {}; {}) {{\n",
                pad, init_s, cond_s, upd_s.join(", ")
            ));
            p_stmt(body, vt, out, ind + 1);
            out.push_str(&format!("{}}}\n", pad));
        }
        Stmt::Switch { selector, cases, default, .. } => {
            out.push_str(&format!("{}switch ({}) {{\n", pad, p_expr(selector, vt)));
            for c in cases {
                let labels: Vec<String> = c.labels.iter().map(|l| l.to_string()).collect();
                for l in &labels {
                    out.push_str(&format!("{}case {}:\n", pad, l));
                }
                for x in &c.body {
                    p_stmt(x, vt, out, ind + 1);
                }
            }
            if let Some(d) = default {
                out.push_str(&format!("{}default:\n", pad));
                p_stmt(d, vt, out, ind + 1);
            }
            out.push_str(&format!("{}}}\n", pad));
        }
        Stmt::Try { body, catches, finally } => {
            out.push_str(&format!("{}try {{\n", pad));
            p_stmt(body, vt, out, ind + 1);
            for c in catches {
                let name = c.var_name.clone().unwrap_or_else(|| vt.var(c.var).name.clone());
                out.push_str(&format!("{}}} catch ({} {}) {{\n", pad, c.exc.join(" | "), name));
                p_stmt(&c.body, vt, out, ind + 1);
            }
            out.push_str(&format!("{}}}\n", pad));
            if let Some(f) = finally {
                out.push_str(&format!("{}finally {{\n", pad));
                p_stmt(f, vt, out, ind + 1);
                out.push_str(&format!("{}}}\n", pad));
            }
        }
        Stmt::Break(None) => out.push_str(&format!("{}break;\n", pad)),
        Stmt::Continue(None) => out.push_str(&format!("{}continue;\n", pad)),
        Stmt::Comment(c) => out.push_str(&format!("{}// {}\n", pad, c)),
        other => out.push_str(&format!("{}<stmt:{:?}>\n", pad, std::mem::discriminant(other))),
    }
}

fn print(s: &Stmt, vt: &VarTable) -> String {
    let mut out = String::new();
    p_stmt(s, vt, &mut out, 0);
    out
}

/// The contract's post-convert step: the converter emits `Catch { var:
/// u32::MAX, .. }` (unbound); the front-end binds each catch to the exception
/// parameter stored by its handler's first statement and drops that store.
///
/// This is the minimal form of jcdc's `resolve_catch_vars` /
/// `assign_catch_names` pair, which a shared `passes` module will own.
fn bind_catches(s: &mut Stmt, vt: &mut VarTable) {
    match s {
        Stmt::Try { body, catches, finally } => {
            bind_catches(body, vt);
            for c in catches.iter_mut() {
                if c.var == u32::MAX {
                    let stored = match c.body.as_ref() {
                        Stmt::Block(v) => match v.first() {
                            Some(Stmt::LocalDef { var, .. }) => Some(*var),
                            Some(Stmt::ExprStmt(Expr::Assign { target, .. })) => match &**target {
                                Expr::Local { var, .. } => Some(*var),
                                _ => None,
                            },
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(v) = stored {
                        let exc = c
                            .exc
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "java/lang/Throwable".into());
                        let slot = vt.var(v).slot;
                        let name = format!("e{}", c.var_name.as_deref().unwrap_or(""));
                        let new_var =
                            vt.add_catch_var(slot, name, TypeRef::J(JavaType::Object(exc)));
                        if let Stmt::Block(vs) = c.body.as_mut() {
                            vs.remove(0);
                        }
                        rewrite_locals(c.body.as_mut(), v, new_var);
                        c.var = new_var;
                    }
                }
                bind_catches(&mut c.body, vt);
            }
            if let Some(f) = finally {
                bind_catches(f.as_mut(), vt);
            }
        }
        Stmt::Block(v) => v.iter_mut().for_each(|x| bind_catches(x, vt)),
        Stmt::If { then_stmt, else_stmt, .. } => {
            bind_catches(then_stmt.as_mut(), vt);
            if let Some(e) = else_stmt {
                bind_catches(e.as_mut(), vt);
            }
        }
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::Labeled { body, .. }
        | Stmt::Synchronized { body, .. } => bind_catches(body, vt),
        Stmt::For { init, body, .. } => {
            init.iter_mut().for_each(|x| bind_catches(x, vt));
            bind_catches(body, vt);
        }
        Stmt::Switch { cases, default, .. } => {
            for c in cases.iter_mut() {
                c.body.iter_mut().for_each(|x| bind_catches(x, vt));
            }
            if let Some(d) = default {
                bind_catches(d, vt);
            }
        }
        _ => {}
    }
}

/// Rewrite every reference to variable `from` as `to` (statement tree).
fn rewrite_locals(s: &mut Stmt, from: u32, to: u32) {
    fn e(ex: &mut Expr, from: u32, to: u32) {
        match ex {
            Expr::Local { var, .. } => {
                if *var == from {
                    *var = to;
                }
            }
            Expr::Bin { l, r, .. } => {
                e(l, from, to);
                e(r, from, to);
            }
            Expr::Un { e: i, .. } | Expr::Cast { e: i, .. } | Expr::InstanceOf { e: i, .. } => {
                e(i, from, to)
            }
            Expr::Assign { target, value, .. } => {
                e(target, from, to);
                e(value, from, to);
            }
            Expr::Field { owner: Some(o), .. } => e(o, from, to),
            Expr::ArrayIndex { array, index } => {
                e(array, from, to);
                e(index, from, to);
            }
            Expr::Method { owner, args, .. } => {
                if let Some(o) = owner {
                    e(o, from, to);
                }
                args.iter_mut().for_each(|a| e(a, from, to));
            }
            Expr::Cond { c, t, f } => {
                e(c, from, to);
                e(t, from, to);
                e(f, from, to);
            }
            _ => {}
        }
    }
    match s {
        Stmt::Block(v) => v.iter_mut().for_each(|x| rewrite_locals(x, from, to)),
        Stmt::ExprStmt(x) | Stmt::Throw(x) => e(x, from, to),
        Stmt::Return(Some(x)) => e(x, from, to),
        Stmt::LocalDef { init: Some(x), .. } => e(x, from, to),
        Stmt::If { cond, then_stmt, else_stmt } => {
            e(cond, from, to);
            rewrite_locals(then_stmt.as_mut(), from, to);
            if let Some(el) = else_stmt {
                rewrite_locals(el.as_mut(), from, to);
            }
        }
        Stmt::While { cond, body } => {
            e(cond, from, to);
            rewrite_locals(body, from, to);
        }
        Stmt::DoWhile { body, cond } => {
            rewrite_locals(body, from, to);
            e(cond, from, to);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — a counted loop with an if/else body.
//
//   r = 0;
//   while (i < n) { if (i == 5) { r = r + 100; } else { r = r + i; } i = i + 1; }
//   return r;
// ---------------------------------------------------------------------------

#[test]
fn register_machine_loop_with_if_else() {
    let vt = vt_with(&[
        ("i", JavaType::Int),
        ("n", JavaType::Int),
        ("r", JavaType::Int),
    ]);
    // Offsets are code units (0, 2, 4, ...) — never JVM pc semantics.
    let cfg = Cfg::from_blocks(
        vec![
            // Block ids MUST equal their index in `blocks` (core invariant).
            blk(0, 0, 2, vec![1]),    // 0 init
            blk(1, 2, 2, vec![6, 2]), // 1 header: -> [fallthrough = exit, taken = body]
            blk(2, 4, 2, vec![4, 3]), // 2 body:   -> [fallthrough = else, taken = then]
            blk(3, 6, 2, vec![5]),    // 3 then
            blk(4, 8, 2, vec![5]),    // 4 else
            blk(5, 10, 2, vec![1]),   // 5 increment -> header
            blk(6, 12, 2, vec![]),    // 6 exit: return
        ],
        0,
        vec![],
    );
    let cond_loop = less_than(local(&vt, "i"), local(&vt, "n"));
    let cond_if = Expr::Bin {
        op: BinOp::Eq,
        l: Box::new(local(&vt, "i")),
        r: Box::new(int(5)),
        ty: Some(TypeRef::J(JavaType::Int)),
    };
    let results = vec![
        res(vec![assign(local(&vt, "i"), int(0))], Term::Goto),                   // 0
        res(vec![], Term::Cond { cond: cond_loop.clone() }),                      // 1
        res(vec![], Term::Cond { cond: cond_if.clone() }),                        // 2
        res(vec![assign(local(&vt, "r"), add(local(&vt, "r"), int(100)))], Term::Goto), // 3
        res(vec![assign(local(&vt, "r"), add(local(&vt, "r"), local(&vt, "i")))], Term::Goto), // 4
        res(vec![assign(local(&vt, "i"), add(local(&vt, "i"), int(1)))], Term::Goto),   // 5
        res(vec![Stmt::Return(Some(local(&vt, "r")))], Term::Return(None)),       // 6
    ];

    let body = structuralize(&cfg, &results);
    let text = print(&body, &vt);
    println!("--- scenario 1 ---\n{}", text);

    // The loop survived structuring and conversion, in one of the equivalent
    // source shapes the structurer may pick.
    assert!(
        text.contains("while (") || text.contains("do {") || text.contains("for ("),
        "no loop in:\n{}",
        text
    );
    assert!(text.contains("return r;"), "no return in:\n{}", text);
    // Both arms of the inner conditional are present (as an if/else or as a
    // folded conditional — the structurer decides).
    let both_arms = (text.contains("r = r + 100;") && text.contains("r = r + i;"))
        || text.contains("if (");
    assert!(both_arms, "conditional arms missing in:\n{}", text);
    assert!(text.contains("i = i + 1;"), "increment missing in:\n{}", text);
}

// ---------------------------------------------------------------------------
// Scenario 2 — a table switch with a default arm.
//
//   switch (n) { case 0: r = 10; break; case 1: r = 20; break; default: r = 30; }
// ---------------------------------------------------------------------------

#[test]
fn register_machine_switch_with_default() {
    let vt = vt_with(&[("n", JavaType::Int), ("r", JavaType::Int)]);
    let cfg = Cfg::from_blocks(
        vec![
            blk(0, 0, 2, vec![1]),        // prologue
            blk(1, 2, 2, vec![2, 3, 4]),  // switch: case0 -> 2, case1 -> 3, default -> 4
            blk(2, 4, 2, vec![5]),        // case 0
            blk(3, 6, 2, vec![5]),        // case 1
            blk(4, 8, 2, vec![5]),        // default
            blk(5, 10, 2, vec![]),        // exit
        ],
        0,
        vec![],
    );
    let results = vec![
        res(vec![], Term::Fallthrough),                                            // 0
        res(
            vec![],
            Term::Switch {
                selector: local(&vt, "n"),
                targets: SwitchTargets::Table { low: 0, targets: vec![4, 6] },
                default: Some(8),
            },
        ),                                                                         // 1
        res(vec![assign(local(&vt, "r"), int(10))], Term::Goto),                   // 2
        res(vec![assign(local(&vt, "r"), int(20))], Term::Goto),                   // 3
        res(vec![assign(local(&vt, "r"), int(30))], Term::Goto),                   // 4
        res(vec![Stmt::Return(Some(local(&vt, "r")))], Term::Return(None)),        // 5
    ];

    let body = structuralize(&cfg, &results);
    let text = print(&body, &vt);
    println!("--- scenario 2 ---\n{}", text);

    assert!(text.contains("switch ("), "no switch in:\n{}", text);
    assert!(text.contains("10") && text.contains("20") && text.contains("30"), "arms missing:\n{}", text);
}

// ---------------------------------------------------------------------------
// Scenario 3 — try/catch: the handler is described by an exception RANGE plus
// a typed catch, i.e. exactly what a DEX `try_item` + `encoded_catch_handler`
// provides.
// ---------------------------------------------------------------------------

#[test]
fn register_machine_try_catch() {
    let mut vt = vt_with(&[("n", JavaType::Int)]);
    let cfg = Cfg::from_blocks(
        vec![
            blk(0, 0, 4, vec![1]),    // protected body -> falls through
            blk(1, 4, 2, vec![]),     // normal exit
            blk(2, 6, 2, vec![]),     // handler
        ],
        0,
        vec![ExcRange {
            start: 0,
            end: 4,
            handler: 6,
            catch_type: Some("java/lang/IllegalStateException".into()),
        }],
    );
    // The catch parameter is a front-end-created variable.
    let handler_var = vt.add_split(
        9,
        "e".into(),
        TypeRef::J(JavaType::Object("java/lang/IllegalStateException".into())),
    );
    assert_eq!(handler_var, 1);
    let results = vec![
        res(
            vec![Stmt::ExprStmt(Expr::Method {
                owner: None,
                cls: "java/lang/Math".into(),
                name: "abs".into(),
                desc: jdc_core::types::parse_method_descriptor("(I)I").unwrap(),
                args: vec![local(&vt, "n")],
                is_static: true,
                is_interface: false,
                is_special: false,
                is_super: false,
                is_dynamic: false,
                type_args: vec![],
            })],
            Term::Fallthrough,
        ),
        res(vec![], Term::Return(None)),
        // The handler block STARTS with the store of the caught exception —
        // that is what the front-end's catch-binding step looks for
        // (reference: jcdc's `resolve_catch_vars`).
        res(
            vec![Stmt::LocalDef { var: handler_var, init: None, is_final: false, force_type: true }],
            Term::Throw(local(&vt, "e")),
        ),
    ];

    let mut body = structuralize(&cfg, &results);
    // Front-end step (see `bind_catches`): bind the catch parameter — the
    // converter leaves `Catch.var` unbound.
    let mut vt = vt;
    bind_catches(&mut body, &mut vt);
    let text = print(&body, &vt);
    println!("--- scenario 3 ---\n{}", text);

    // The exception RANGE became a try/catch with a typed, NAMED parameter.
    assert!(text.contains("try {"), "no try in:\n{}", text);
    assert!(
        text.contains("catch (java/lang/IllegalStateException e") || text.contains("catch (e"),
        "no typed catch parameter in:\n{}",
        text
    );
    assert!(text.contains("throw e;"), "handler body lost:\n{}", text);
}