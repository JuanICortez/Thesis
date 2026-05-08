# `csearch` — Project Plan

A semantic code-search tool for C, built on slotted e-graphs. Rust core
with a CLI for metrics work and PyO3 bindings as the eventual primary
distribution.

This document captures the agreed structure as of 2026-05-08. It builds
on `Slotted_Egraph_Implementation_Test.md` (prototype summary) and
`Slotted_Egraph_Implementation_Plan.md` (library/tool split).

---

## Decisions

- **Fork strategy.** `slotted-egraphs` is forked permanently and lives
  as a workspace member. No upstreaming pressure; the upstream repo is
  low-activity. Library improvements (Part A in the implementation plan)
  land directly in the vendored fork.
- **Distribution.** Primary target is a Python library via PyO3. A
  Rust CLI is also kept around for ad-hoc metrics and benchmarking.
- **MVP scope.** Parse simple `.c` files end-to-end. No
  preprocessor support, no pointers/structs, no `for`-loop variants
  beyond what the lowering can normalize.
- **Pipeline shape.** Tree-sitter → simplified-C AST → `RecExpr<CSubset>`
  → `EGraph`. RecExpr is kept as an explicit intermediate for
  testability and pretty-printing. No S-expression round-trip in the
  program-input path; S-expressions remain only as the pattern syntax
  for queries.

---

## Repository layout

Single Cargo workspace, fork vendored as a workspace member:

```
csearch/                              ← repo root, Cargo workspace
├── Cargo.toml                        ← [workspace] members = [...]
├── README.md
├── crates/
│   ├── slotted-egraphs/              ← vendored fork
│   │   └── (existing tree + Part A additions)
│   ├── csearch-core/                 ← Rust library: language, analyses, search API
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── lang.rs               ← CSubset (define_language!)
│   │   │   ├── rewrite.rs            ← arithmetic / boolean / control-flow rewrites
│   │   │   ├── context_rewrites.rs   ← guarded sequence-spanning rewrites
│   │   │   ├── const_fold.rs
│   │   │   ├── span_analysis.rs      ← richer Span (file_id, byte range, line/col)
│   │   │   ├── lower/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── tree_sitter.rs    ← C source → CAst
│   │   │   │   └── abt.rs            ← CAst → (RecExpr<CSubset>, SpanTree)
│   │   │   ├── codebase.rs           ← CodeBase, Match, Location
│   │   │   └── pattern.rs            ← pattern parsing (s-expr) for queries
│   │   └── tests/
│   ├── csearch-cli/                  ← thin Rust binary for metrics / ad-hoc use
│   │   └── src/main.rs
│   └── csearch-py/                   ← PyO3 bindings (cdylib + maturin)
│       ├── Cargo.toml
│       ├── pyproject.toml
│       └── src/lib.rs
└── examples/
    └── c-samples/                    ← small .c files for tests + benches
```

### Why this shape

- `csearch-core` owns everything language-specific. Both the CLI and
  the Python bindings are thin shells around it, so the metrics path
  and the eventual Python API can't drift.
- `csearch-py` is a separate crate rather than a feature flag on
  `csearch-core`. Keeps `cargo test` clean of Python build deps and
  lets `maturin develop` operate on just that crate.
- Patterns and lowered programs both end up as `RecExpr<CSubset>`, so
  `pattern.rs` is thin — it reuses the library's S-expression parser.

---

## Pipeline

```
.c file
   │  tree-sitter-c
   ▼
CST  ──► CAst (rich, trimmed C; uniform node type; span on every node)
                │  abt.rs: hoist locals, normalize loops, lower ++/--, etc.
                ▼
        (RecExpr<CSubset>, SpanTree)
                │  library's add_expr_with_spans
                ▼
         EGraph<CSubset, SpanAnalysis>
                │  saturate with rewrites
                ▼
         search(pattern) → Vec<Match>
```

Stages:

1. **`lower/tree_sitter.rs`** — `parse_c(src) -> CAst`. Pure
   tree-sitter-out-of-the-CST work. CAst still carries C-isms (`For`,
   `DoWhile`, `++`, compound assigns, declarations with init).
2. **`lower/abt.rs`** — `lower(CAst) -> (RecExpr<CSubset>, SpanTree)`.
   Normalization: hoist locals into `Fun`'s `MultiBind`, `for` →
   `seq + loop`, `++` → `assign + add`, etc.
3. **Insertion** uses the library's `add_expr_with_spans` (no
   csearch-specific insertion path).

The RecExpr-as-intermediate gives snapshot tests on lowering output,
debug pretty-printing, and a single insertion code path shared with the
prototype's test suite.

---

## Library work (in the vendored fork)

These are the items from `Slotted_Egraph_Implementation_Plan.md` Part A
that need to land before `csearch-core` can lean on the library.

- **A.1 — `MultiBind<T>`.** Promote from prototype tests into
  `src/lang.rs`.
- **A.2 — `semantic_search` + `ematch_all_with_roots`.** Promote into
  a new `src/search/` module. Removes the `pattern_root_eclass`
  workaround.
- **A.3 — Position-tracking parser + `SpanTree` + span-aware insertion.**
  `parse_with_spans`, `SpanTree`, and `add_expr_with_spans` move into
  the library, generic over the consumer's position type. The library
  keeps `Span` minimal (byte offsets); `csearch-core` instantiates with
  a richer `(FileId, Range<usize>, line, col)`.

A.4 (`Searcher`/`Applier`) and A.5 (`Analysis` trait improvements) stay
deferred — neither is needed to ship the MVP.

---

## Milestones

Each milestone is independently demoable.

### M1 — Library upstreams

Land A.1, A.2, A.3 in the vendored fork. Verified by porting the
existing `tests/c_subset/` tests to use the new public API.

### M2 — `csearch-core` skeleton

Move `CSubset`, rewrites, `ConstFold`, `SpanAnalysis` out of the test
module into the new crate. All existing tests pass relocated. No new
functionality.

### M3 — C lowering pipeline

Implement `lower/tree_sitter.rs` + `lower/abt.rs`. MVP scope:

- Function definitions, `int` parameters and locals.
- Arithmetic and boolean expressions, comparisons.
- `if`, `while`, `return`, plain `assign`.
- `for` lowered to `while`; `++`/`--`/compound assigns to plain `assign`.
- Skip: pointers, structs, arrays, preprocessor, multi-file.

### M4 — `CodeBase` API + CLI

High-level wrapper. See "CodeBase API" section below for the full
shape. Plus a `csearch` CLI binary that takes a `.c` file and a
pattern and prints grep-style output. This is also the metrics tool.

### M5 — PyO3 bindings

Expose `CodeBase`, `Match`, `Location`. Ship as a wheel via maturin.
Small Python surface: `CodeBase()`, `.add_file()`, `.saturate()`,
`.search()`.

---

## CodeBase API (M4)

The user-facing surface for the Rust core. The CLI and PyO3 bindings
are both thin wrappers over this.

### Decisions

- **Single file only for MVP.** No `FileId`, no source map, no
  directory loading. Multi-file is a possible future extension.
- **Search works pre- and post-saturation.** Pre-saturation matches
  are purely structural; post-saturation matches are modulo whichever
  rewrites the user picked. Same code path, no auto-saturation.
- **Bundled rule sets, with BYO escape hatch.** `RuleSet::Default`
  covers the prototype's full rewrite set; `RuleSet::Custom(...)` for
  advanced use.
- **One query at a time.** No e-class id leaks into `Match`; no
  follow-up queries based on prior results. No `compile_pattern` —
  patterns are parsed per `search` call. Revisit if interactive use
  becomes painful.
- **Pretty-printing as S-expressions.** `Match::matched_expr` is the
  S-expression of the smallest representative. C-source reconstruction
  is post-MVP.
- **Eager construction.** `load` runs tree-sitter, lowering, and
  e-graph insertion before returning. No separate `add_file` step.

### Surface

```rust
pub struct CodeBase {
    egraph: EGraph<CSubset, SpanAnalysis>,
    source: String,
    path: PathBuf,
    saturated: bool,
}

impl CodeBase {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError>;
    pub fn from_source(name: &str, src: &str) -> Result<Self, LoadError>;

    pub fn saturate(&mut self, config: &SaturationConfig) -> SaturationReport;
    pub fn is_saturated(&self) -> bool;

    pub fn search(&self, pattern: &str) -> Result<Vec<Match>, PatternError>;

    pub fn path(&self) -> &Path;
    pub fn source(&self) -> &str;
    pub fn stats(&self) -> EGraphStats;
}

pub struct SaturationConfig {
    pub rules: RuleSet,
    pub iter_limit: usize,
    pub node_limit: usize,
    pub time_limit: Duration,
}
impl Default for SaturationConfig { /* sensible defaults */ }

pub enum RuleSet {
    Default,                             // arithmetic + boolean + control + context
    Arithmetic,                          // just the arithmetic rewrites
    Custom(Vec<Rewrite<CSubset, SpanAnalysis>>),
}

pub struct SaturationReport {
    pub iterations: usize,
    pub stop_reason: StopReason,         // Saturated | IterLimit | NodeLimit | TimeLimit
    pub nodes_before: usize,
    pub nodes_after: usize,
    pub elapsed: Duration,
}

pub struct Match {
    pub matched_expr: String,            // S-expression of smallest representative
    pub locations: Vec<Location>,        // distinct source spans (post-rewrite-merge)
}

pub struct Location {
    pub start: usize, pub end: usize,    // byte offsets
    pub line: usize, pub col: usize,     // 1-indexed
}
```

Notes:

- `Location` does not carry a path. With a single file, the path comes
  from `CodeBase::path()`. Output formatting code joins them.
- `is_saturated()` is largely redundant under single-file semantics
  but kept so the CLI can render an "(unsaturated)" hint.
- A `Match` carries `Vec<Location>` because one e-class can correspond
  to many source positions, especially after rewrites merge equivalent
  fragments. This is the prototype's documented behavior.

---

## Out of scope for MVP

- Multi-file `Span` linking beyond `(FileId, byte range)`.
- Incremental updates on file change (would need a "remove span" hook
  on the analysis).
- C-like pattern syntax (S-expression patterns are sufficient for a
  research/metrics tool; revisit post-MVP).
- Pointers, structs, arrays.
- Preprocessor handling.
- Program-permutation rewrites and side-effect analysis.

These are tracked in `Slotted_Egraph_Implementation_Plan.md` Part C.

---

## CAst node set (M3)

The intermediate between tree-sitter's CST and `RecExpr<CSubset>`.
Guiding principle: CAst keeps anything a human would recognize as
"the same C"; the abt pass does all semantic-preserving normalization.
That makes each lowering step independently testable.

### Decisions

- **`BoolLit(true/false)` survives into CAst.** Maps to
  `Symbol(true/false)` in the abt pass. Tree-sitter pass recognizes
  `<stdbool.h>`-style `true`/`false`; everything else stays int.
- **`Call` survives into CAst, single-argument only.** No
  interprocedural analysis at MVP. Multi-arg calls error during
  lowering. User-defined rewrites can later substitute calls with
  expected values when needed.
- **`++`/`--` only as statements, not subexpressions.** Pre vs. post
  differs in expression value, which we don't model. Statement-level
  use is unambiguous.
- **Block scoping forbids shadowing.** `{ int x; { int x; } }` errors
  during lowering. Renaming-on-shadow is a post-MVP add.

### Node set

```rust
pub struct CAst {
    pub functions: Vec<CFunction>,
}

pub struct CFunction {
    pub name: String,
    pub params: Vec<CParam>,    // int-typed only at MVP
    pub body: CBlock,
    pub span: Span,
}

pub struct CParam { pub name: String, pub span: Span }
pub struct CBlock { pub stmts: Vec<CStmt>, pub span: Span }

pub enum CStmt {
    Decl    { name: String, init: Option<CExpr>, span: Span },
    Assign  { target: String, op: AssignOp, value: CExpr, span: Span },
    If      { cond: CExpr, then_b: CBlock, else_b: Option<CBlock>, span: Span },
    While   { cond: CExpr, body: CBlock, span: Span },
    DoWhile { body: CBlock, cond: CExpr, span: Span },
    For     { init: Option<Box<CStmt>>, cond: Option<CExpr>,
              step: Option<CExpr>, body: CBlock, span: Span },
    Return  { value: Option<CExpr>, span: Span },
    Block   (CBlock),
    ExprStmt(CExpr),
}

pub enum AssignOp { Plain, Add, Sub, Mul }       // =, +=, -=, *=

pub enum CExpr {
    Num(i32, Span),
    BoolLit(bool, Span),
    Var(String, Span),
    BinOp   { op: BinOp, lhs: Box<CExpr>, rhs: Box<CExpr>, span: Span },
    UnaryOp { op: UnaryOp, operand: Box<CExpr>, span: Span },
    Call    { name: String, args: Vec<CExpr>, span: Span },  // single-arg at MVP
    IncDec  { op: IncDec, fix: Fix, target: String, span: Span },
}

pub enum BinOp { Add, Sub, Mul, Eq, Neq, Lt, Gt, Le, Ge, And, Or }
pub enum UnaryOp { Neg, Not }
pub enum IncDec  { Inc, Dec }
pub enum Fix     { Pre, Post }
```

### Tree-sitter pass strips silently

Pure noise for our purposes — never makes it into CAst:

- Comments, whitespace, parenthesized-expression wrappers
- Type qualifiers (`const`, `volatile`)
- Storage classes (`static`, `extern`)
- Casts (MVP is int-only, so casts are no-ops)
- `typedef`, `#include`, `#define` (preprocessor out of scope)

### abt pass normalizations

| CAst | CSubset |
|---|---|
| `Num`, `Var` | `Num`, `Var` (slot lookup) |
| `BoolLit(true/false)` | `Symbol(true/false)` |
| `BinOp::{Add,Sub,Mul,Lt,Eq,And,Or}` | direct |
| `BinOp::Neq(a,b)` | `Not(Eq(a,b))` |
| `BinOp::Gt(a,b)` | `Lt(b,a)` |
| `BinOp::Le(a,b)` | `Not(Lt(b,a))` |
| `BinOp::Ge(a,b)` | `Not(Lt(a,b))` |
| `UnaryOp::{Neg,Not}` | direct |
| `Decl{name, init: Some(e)}` | `assign($name, e)` |
| `Decl{name, init: None}` | omitted (slot bound, no initial value) |
| `Assign{op: Plain}` | `assign($x, e)` |
| `Assign{op: Add}` | `assign($x, add(var $x, e))` (Sub/Mul similar) |
| `IncDec` (statement) | `assign($x, add(var $x, ±1))` |
| `If{else_b: Some}` | `Ite(cond, then_seq, else_seq)` |
| `If{else_b: None}` | `Ite(cond, then_seq, nop)` |
| `While` | `Loop(cond, body_seq)` |
| `DoWhile{body, cond}` | `Seq(body_seq, Loop(cond, body_seq))` |
| `For{init, cond, step, body}` | `Seq(init?, Loop(cond ?? true, Seq(body_seq, step?)))` |
| `Return{value: Some(e)}` | `Ret(e)` |
| `Return{value: None}` | `Ret(Symbol(nop))` |
| `Block` | flatten into surrounding `Seq` chain |
| `Call` | `Call(name, arg)` — single arg at MVP |

### Local hoisting

The abt pass walks each function body once up-front to collect `Decl`
names plus parameter names. These become the `Fun`'s `MultiBind` slots.
A `name → Slot` map is threaded through expression lowering so
`Var(x)` resolves to the right slot. References to undeclared names
error at lowering time.

---

## Open design questions

1. **`CodeBase` saturation defaults.** Specific iteration / node /
   time limits for `SaturationConfig::default()`. Pin down once we
   have realistic input sizes.
2. **Python ergonomics.** Whether `Match`/`Location` should be plain
   dataclasses on the Python side or keep their PyO3-wrapped form.

These get nailed down at the start of the milestone that touches them.
