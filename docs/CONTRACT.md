# Front-end contract

What a front-end must provide to use this crate, and what it may rely on.
Everything here is enforced by `tests/toy_frontend.rs`, which plays the role of
a register-machine front-end and never touches a class file.

## 1. The CFG

Build one `Cfg` per method body with `Cfg::from_blocks(blocks, entry, exc_ranges)`.

**Block invariants** (the structurer addresses blocks by id):

| Field | Obligation |
|---|---|
| `id` | **Must equal the block's index in `blocks`.** |
| `start` / `end` | Machine offsets in `u32` (JVM pc, DEX code units). `end` exclusive. |
| `ins_len` | Number of instructions (or code units) in the block. `0` marks a structural stub with no code of its own (handler entry, empty range) — the structurer treats those differently. |
| `succ` | For a conditional terminator: **`succ[0]` is the fall-through, `succ[1]` the taken edge.** The structurer may reorder branches by condition polarity, but it starts from this convention. |
| `pred`, `handlers` | Derived by `from_blocks` — leave empty. |

**Exception ranges** map directly onto the machine's own tables:

| This crate | JVM | DEX |
|---|---|---|
| `ExcRange { start, end, handler, catch_type }` | `exception_table` entry | `try_item` + `encoded_catch_handler` |
| `catch_type: None` | `finally`-style catch-all | catch-all handler |

`from_blocks` turns a range into exception edges from every block inside it and
fills each handler's `handlers` list — the structurer's try reconstruction reads
only those.

## 2. Block results

One `BlockResult` per block, indexed by block id:

| Field | Obligation |
|---|---|
| `stmts` | The block's statements, in order, **excluding** the terminator. |
| `term` | How the block ends. `Term::Cond { cond }` carries the *condition expression*; `Term::Switch { selector, targets, default }` carries the decoded key range **and the default target's offset** (JVM: the switch payload's default; DEX: the `packed-switch`/`sparse-switch` payload). |
| `out_stack` | Value-machine leftovers at block exit (JVM operand stack; a register machine leaves it empty). Only the front-end's own merge handling reads it. |

Statement trees must be **Java-shaped** — everything downstream assumes it:

* `Stmt::ExprStmt(e)` only for expressions that are legal Java statement
  expressions (call, assignment, new, inc/dec). A bare field read, a cast, or
  `Outer.this` is **not**; materialize such values into a synthetic local
  instead (`discardedN` in jcdc) so trapping reads still happen.
* `Stmt::LocalDef` declares; re-assignment after that is `Expr::Assign`.
* `Stmt::Return`/`Throw` are statements; the matching `Term` says the block ends
  there.

## 3. Variables

`VarTable` is yours to fill: the core only reads it (`vt.var(id).name` /
`.ty`, parameter flags, `stack_vars` for hoisting decisions).

* Every `Expr::Local { var }` must have an entry (`add_split` / `add_catch_var`
  are the constructors for synthetic ones).
* Names must be unique per method scope; the emitter prints them verbatim.
* Types may be approximated (`TypeRef::J`); the passes refine them where the
  IR proves more.

## 4. Ctx: metadata questions

Implement `jdc_core::ctx::Ctx` once. It is deliberately *semantic* — the core
never sees a constant pool, a `Signature` attribute or an `InnerClasses` table:

| Method | Meaning |
|---|---|
| `class_name` | The class currently being emitted. |
| `source_level` | Language-level gate (JVM: class-file major; DEX: the level you target). `52` = Java 8 features available. |
| `find_outer` | Nesting evidence (`None` for top-level, including literal-`$` names). |
| `family` | Nested-class family (member/anonymous/local/lambda). |
| `nested_is_static`, `class_has_this0`, `outer_param_via_super` | Inner-class construction rules. |
| `is_subtype_of`, `is_interface`, `is_sealed`, `super_name`, `has_class` | Type-graph queries. |
| `class_bases` | Interfaces + superclass (for anonymous-type rendering). |
| `field_flags`, `method_flags`, `declares_field`, `declares_method_named` | Member lookups (shadowing, owner casts). |
| `class_type_params`, `class_declares_generics` | Class-level generics (diamond, witness substitution). |
| `is_generic_call`, `generic_call_formals` | Calls whose inferred type arguments the source must spell out. |
| `polymorphic_ret_cast` | Self-covariant returns (`Stream` over `BaseStream`). |
| `ctor_formals_by_arity`, `ctor_param_types` | `new`-site argument typing. |
| `sam_ret_cast` | Functional-interface SAM return resolution. |
| `local_class_internal` | `$`-marker → local-class name. |
| `nested_method` | **The big one**: decompile a nested method body (lambda / anonymous-class method / local-class method) *including the front-end's idiom recovery*. Takes the `LambdaExpr` (which carries the impl owner/name/descriptor, the SAM parameter names, the capture expressions and any front-end annotations such as capture snapshots) plus the enclosing `VarTable` — the lexical scope the body is printed in. |

Every method has a conservative default; `NullCtx` answers "unknown" to all of
them, and the core degrades gracefully (no nesting assumptions, no generic
casts, no nested bodies).

**Emission seams** (what the `emit` port needs; see `wip/emit.rs.partial`):
`source_level` (the old `major_version < 52` gates), `has_class` /
`find_outer` / `class_bases` (name shortening and `$`-literal detection),
`ctor_param_types` and `ctor_formals_by_arity` (`new`-site typing),
`sam_ret_cast` / `class_type_params` / `polymorphic_ret_cast` (witness casts),
`field_flags` / `method_flags` (shadowing, owner casts), `is_interface` /
`is_sealed` / `is_subtype_of` (cast elision), and `nested_method` (lambda
bodies — this one call replaces ~130 lines of front-end pipeline that used to
live inside the printer).

## 5. Post-convert obligations

The converter hands back a statement tree that still needs front-end work:

1. **Bind catch parameters.** `Catch.var == u32::MAX` means unbound; the
   handler's first statement is the store of the caught object. Allocate a
   catch variable (`VarTable::add_catch_var`), drop that store, rewrite the
   handler body's references, set `catch.var`. jcdc's `resolve_catch_vars` +
   `assign_catch_names` are the reference implementation; `bind_catches` in
   `tests/toy_frontend.rs` is the minimal form.
2. **Run the shared refinements** once the `passes` module lands (jcdc
   `method.rs`): diamond folding, booleanization, slot splitting, duplicate
   declaration dedupe, dead-store pruning, label pruning, wrap-returns,
   exception rethrow merging, … 121 of its 141 top-level items are already
   machine-free; the ~20 metadata-readers go behind `Ctx`.
3. **Front-end idioms** stay yours: javac's `try`-with-resources lowering, d8's
   `StringBuilder` chains, `access$` / `-$$Nest$` accessors, enum switch maps,
   Kotlin metadata — all of it operates on the same IR and is invoked from
   `Ctx::nested_method` or your own pass pipeline.

## 6. What the core guarantees

* **No machine knowledge**: nothing here decodes an instruction, reads a
  constant pool, or assumes an operand stack.
* **Determinism**: identical input IR produces identical regions and statement
  trees; both structurer paths (`walk` default, `SESE` via `JCDC_SESE=1`) are
  exercised by the test suite.
* **Failure tolerance**: unknown metadata degrades rendering (raw names, erased
  types, `/* unavailable */` stubs) instead of failing.