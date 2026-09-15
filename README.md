# jdc-core

Machine-neutral decompiler core: **IR, CFG structuring, statement conversion,
Java emission** — extracted from [jcdc](https://github.com/ejfkdev/jcdc) so that
any Java-family front-end (JVM `.class` files, **DEX/APK**, and whatever comes
next) can share the hard half of a decompiler.

```
  front-end (machine-specific)                jdc-core (machine-neutral)
  ───────────────────────────                ──────────────────────────
  parse bytes / decode                       cfg::Cfg         blocks + edges
  build expressions per block   ───────────► ir::BlockResult  stmts + term
  synthesize a VarTable                      structure::Structurer / sese
  machine idioms (javac / d8 / R8 / ...)     convert::Converter
                                 └─────────► passes::*        shared refinements
                                             emit::Printer    Java source text
```

A decompiler is a pipeline. Only its **first two steps** know anything about the
source machine — that an operand stack exists, that registers hold values, how
instructions are encoded. Everything after them reasons about *Java*: loops,
conditionals, switches, exception regions, statement trees, expressions with
precedence. That is the part this crate provides, and it is the part that took
the most engineering in jcdc (structuring alone is ~12k lines that were tuned
against a 21-release JDK corpus and a 48k-class third-party corpus).

## What is in the box

| Module | What it is |
|---|---|
| `types` | Descriptor + generic-signature model (`Lcom/foo/Bar;`, `[I`, `(ILjava/lang/String;)V`), access flags. JVM and DEX spell types identically, so one model serves both. |
| `ir` | `Expr` / `Stmt` trees, `Term` / `BlockResult` (how a block ends, what it produced). |
| `cfg` | `Cfg` + `Block` with `u32` offsets (JVM pcs, DEX code units), successor/handler derivation, `from_blocks`. |
| `structure` | The structurer: loops, conditionals, switches, try/exception regions, shared-tail copies, parked chains. |
| `sese` | The SESE/dominator-tree structurer (second path over the same IR). |
| `convert` | `Region` → `Stmt` (break/continue/label resolution, case groups, catches). |
| `var` | `VarTable`: the variable identities the emitter prints by. |
| `analysis`, `typeutil` | Statement predicates and pure type helpers. |
| `ctx` | `Ctx` — the *only* thing a front-end must implement. |
| `dbg` | Cached debug-env macros (`dbg_flag!`, `dbg_value!`). |

## What a front-end does

1. Parse its container, decode instructions, and **build expressions** into
   `BlockResult`s (one per basic block).
2. Assemble a `Cfg` (`Cfg::from_blocks` derives predecessors, handler lists and
   exception edges).
3. Fill a `VarTable` (JVM: from `LocalVariableTable`s; DEX: synthesized from
   register live ranges — release APKs usually carry no names at all).
4. Implement `Ctx` for metadata questions (nesting, subtypes, signatures,
   nested method bodies).
5. Run the structurer + converter, then the shared refinement passes.
6. Print: `emit::Printer` (front-end driven — Java syntax is machine-neutral).

See **[docs/CONTRACT.md](docs/CONTRACT.md)** for the exact obligations and the
conventions the structurer relies on (successor order, block-id == index,
statement-tree shape), and **[tests/toy_frontend.rs](tests/toy_frontend.rs)**
for a complete non-JVM front-end: it hand-builds CFGs for a loop with an
if/else, a table switch and a try/catch, runs both structurer paths, and prints
Java.

```sh
cargo test                       # unit tests + the toy front-end, both paths
JCDC_SESE=1 cargo test           # exercises the SESE structurer instead
JCDC_DBG_REGIONS=1 ...           # region dump (with SESE)
```

## Status

Ported and green (≈15k lines): types, IR, CFG, `structure`, `sese`, `convert`,
`var`, `analysis`, `typeutil`, `ctx`.

Still to port from jcdc, in order of value:

1. **`emit`** (≈3.3k lines) — the printer is written against the `Ctx` surface
   already defined here; the WIP port lives in `wip/emit.rs.partial` and its
   remaining seams are inventoried in `docs/CONTRACT.md` §4. Nothing in it needs
   new abstractions, only the mechanical rewiring of ~10 class-metadata helpers
   onto `Ctx`.
2. **`passes`** (jcdc `method.rs`, ≈11.5k lines) — 121 of its 141 top-level
   items are already free of machine types and move as-is (diamond folding,
   booleanization, dead-store pruning, duplicate-declaration dedupe, label
   pruning, …); ~20 read class metadata and go behind `Ctx`. Until they move,
   a front-end runs its own subset — the minimum is catch-parameter binding
   (see the `bind_catches` reference in the toy test).

## Origin and licence

Extracted from [jcdc](https://github.com/ejfkdev/jcdc) (MIT, same author). The
structuring code carries the debugging history of that project in its comments —
they are kept deliberately: each one records a real-world shape that broke it.
MIT.