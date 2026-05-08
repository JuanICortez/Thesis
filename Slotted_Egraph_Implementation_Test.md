# `c_subset` — Implementation Summary

A prototype for a semantic code search tool over a simplified C-like
language, built on top of slotted e-graphs. Lives entirely in the test
module — no changes to `src/`.

---

## Design fundamentals

### Mental model: program-as-is, not value-equivalence

The e-graph stores **program structure**, not claims about runtime values.
`(var $x)` is the *name* `$x`, not "x's current value". E-class equality
means "interchangeable program fragments under our rewrite rules" — never
"produces the same runtime value."

**Why.** This sidesteps the central tension between e-graphs (which
internalize equality) and imperative programs (which mutate). The naive
SSA encoding would have to fight `(var $x) = 1` colliding with
`(var $x) = x + 1`. With the program-as-is model, those are just nodes
in a sequence, not equality claims; nothing collides.

**How.** Rewrites become *context-sensitive* — they only fire when they
see enough of the surrounding program to be sound. The e-graph itself
doesn't try to evaluate variable expressions on its own.

### No `Let` node

All variables (parameters and locals) are bound at the function level via
`Fun`'s `MultiBind`. There's no expression-level `let`.

**Why.** Matches C semantics — local scope is effectively function-wide,
and compilers hoist allocations to the function prologue. One binding
mechanism instead of two.

**How.** The Tree-sitter → ABT pass is responsible for hoisting locals
into the function signature. Inside the body, all variable references
go through `(var $x)` where `$x` was bound by the enclosing `Fun`.

### Loops as direct while-statements

```rust
Loop(AppliedId, AppliedId) = "loop",   // (loop cond body)
```

**Why.** Tracks C source structure directly. Multi-variable mutation
inside the body works with no extra machinery — just sequence multiple
`assign` nodes.

**How.** `Loop` binds nothing of its own; the slots used in `cond` and
`body` are bound by an outer `Fun`. There's no SSA, no phi nodes, no
recursive-function encoding.

---

## What's implemented

### `mod.rs` — the `CSubset` language

The core enum with `define_language!`:

| Variant | Purpose |
|---|---|
| `Var(Slot)`, `Num(i32)` | Atoms |
| `Add`, `Sub`, `Mul`, `Neg` | Arithmetic |
| `Eq`, `Lt` | Comparisons |
| `Not`, `And`, `Or` | Boolean logic |
| `Ite(c, t, f)` | If-then-else |
| `Seq(a, b)` | Statement sequencing |
| `Ret(e)` | Return |
| `Fun(name, MultiBind<body>)` | Function declaration |
| `Call(name, arg)` | Function call |
| `Assign(slot, value)` | Mutation (just a node — no equality claim) |
| `Loop(cond, body)` | While-loop |
| `Symbol(Symbol)` | Identifiers / nullary constants (`true`, `false`, `nop`) |

**Why.** Covers a meaningful subset of C while staying simple enough
for prototyping. Every variant either matches C source directly or is
a normalization target (e.g. all loops → `Loop`).

---

### `multi_bind.rs` — the `MultiBind<T>` binder

```rust
pub struct MultiBind<T> {
    pub slots: Vec<Slot>,
    pub elem: T,
}
```

**Why.** The library's `Bind<T>` binds exactly one slot. We need to bind
multiple at once (function parameters + hoisted locals) without nested
`Bind<Bind<...>>` chains.

**How.** Implements `LanguageChildren`, exposing all bound slots in
`all_slot_occurrences_*` but filtering them out of
`public_slot_occurrences_*` so the body's references to bound slots
become private. `weak_shape_impl` renames each bound slot to a fresh
canonical numeric slot, processes the body, then restores. Parser
support: consume all leading `Slot` syntax elements as bound names,
then defer the rest to the inner type.

---

### `const_fold.rs` — `ConstFold` analysis

```rust
pub enum Const { Int(i32), Bool(bool) }
pub struct ConstFold;
impl Analysis<CSubset> for ConstFold {
    type Data = Option<Const>;
    // ...
}
```

**Why.** Reduce e-graph growth from purely-numeric expressions like
`2 + 3` — fold them to `5` instead of carrying around all algebraically
equivalent forms.

**How.** `make` walks the e-node, requesting `Option<Const>` from each
child's analysis data. Returns `Some(...)` only when all operands are
constants. `merge` keeps any known value (asserts agreement). `modify`
adds a literal node (`Num` or `Symbol`) to the e-class when a constant
is computed, so future patterns can match against the literal directly.

---

### `rewrite.rs` — algebraic + control-flow rewrites

A list of `Rewrite<CSubset, N>` that's generic over the analysis type, so
the same rules work whether we use `()` or `ConstFold`.

| Category | Rules |
|---|---|
| Arithmetic | `add-comm`, `add-assoc1/2`, `mul-comm`, `neg-neg`, `sub-to-add-neg`, `add-zero`, `mul-one`, `mul-zero`, `sub-self` |
| Boolean | `not-not`, `and-comm`, `or-comm`, `and-true`, `and-false`, `or-true`, `or-false`, `de-morgan-and`, `de-morgan-or` |
| Control flow | `ite-true`, `ite-false`, `ite-not` |
| Sequencing | `seq-nop-left`, `seq-nop-right` |

**Why.** Establish baseline equivalences. Most are forward-direction only
(no `add-zero-rev`-style explosion-inducing rules) to keep the e-graph
size manageable.

**How.** Plain pattern → pattern rewrites. The kept identities require
contextual matching (one literal `0`/`1`/`true` etc.) so they don't fire
gratuitously and don't duplicate work `ConstFold` already does.

---

### `c_loop.rs` — `Loop` semantics

Tests for the `Loop(cond, body)` shape, including alpha-equivalence
across loop variable names and multi-variable mutation in the body.

**Why.** Loops are the headline example of where the design choices
(program-as-is, no SSA, no phi nodes) pay off. Tests document and lock
in the intended semantics.

---

### `search.rs` — `semantic_search`

```rust
pub fn semantic_search<L, N>(
    eg: &EGraph<L, N>,
    pattern: &Pattern<L>,
) -> Vec<SearchResult<L>>
```

**Why.** The library's `ematch_all` returns substitutions (one per
match), but for code search we need to know *which e-class* matched and
get a concrete expression back.

**How.** Wraps `ematch_all`. For each substitution, reconstruct the
matched e-class by instantiating the pattern root and looking it up in
the e-graph. Dedupe by canonical e-class id. Extract a smallest
representative via `Extractor<AstSize>`.

**Important nuance.** Two source-level occurrences that are structurally
equivalent collapse into a single e-class — so this returns
*distinct e-classes*, not *source-level occurrences*. The `spans.rs`
module recovers occurrence-level info via source positions.

---

### `context_rewrites.rs` — Tier-4 guarded rewrites

Context-sensitive ("guarded") rewrites that match patterns spanning
multiple statements connected by `seq`. Sound by virtue of the pattern
itself carrying enough surrounding context.

| Rewrite | Pattern | Side condition |
|---|---|---|
| `seq-assoc-l/r` | `(seq (seq A B) C) ↔ (seq A (seq B C))` | none — always sound |
| `adj-init-increment` | `(seq (assign $1 ?init) (assign $1 (add (var $1) ?n))) → (assign $1 (add ?init ?n))` | `$1` not free in `?n` |
| `increment-chain` | `(seq (assign $1 (add (var $1) ?a)) (assign $1 (add (var $1) ?b))) → (assign $1 (add (var $1) (add ?a ?b)))` | `$1` not free in `?b` |

**Why.** Validates the program-as-is + guarded-rewriting design. Each
rewrite would be unsound as a global identity; what makes it sound is
that the pattern includes the assigning-then-using context, AND a side
condition prevents the unsound shape (where the inlined sub-expression
would read a different value of the variable).

**How.** Plain `Rewrite::new_if` patterns with closures that check
slot membership in the bound substitution variables. The side conditions
use the same `subst[v].slots().contains(&Slot::numeric(1))` idiom as the
library's existing `let-unused`.

**A subtlety we hit.** The `increment-chain` rule's side condition
initially required *both* `?a` and `?b` to be free of `$1`. Working out
the semantics carefully revealed that `?a` *can* reference `$1` — it's
evaluated with the same `x` in both the original and rewritten form.
Only `?b` needs the guard, because in the original it sees the post-`?a`
value of `x` while the rewrite has it see the pre-`?a` value. Designing
context-sensitive rewrites requires being precise about which sub-term
sees which intermediate state.

**Composition test.** `chained_increments_collapse` proves that
`x = 0; x = x+1; x = x+2 ≡ x = 3` reaches under combinations of
`seq-assoc`, `adj-init-increment`, `increment-chain`, and `ConstFold`.
Multiple rewrites firing in concert reduce the whole chain to a literal.

---

### `spans.rs` — source-position tracking

The "where in the source did this match come from?" layer. Built on the
`Analysis` trait so positions automatically propagate through e-class
merges.

```rust
pub struct Span { pub start: usize, pub end: usize }
pub struct SpanTree { pub span: Span, pub children: Vec<SpanTree> }
pub struct SpanAnalysis;
impl Analysis<CSubset> for SpanAnalysis {
    type Data = SmallHashSet<Span>;
    fn make(...) -> ... { SmallHashSet::default() }
    fn merge(l, r) -> ... { /* union */ }
}
```

**Why.** Users want to know *where* matches occurred. The e-graph
collapses equivalent expressions, so an external map would have to be
manually re-canonicalized after every merge. Storing spans as analysis
data means **the e-graph maintains the position set automatically** —
when a rewrite merges two e-classes, `Analysis::merge` unions their
span sets.

**How.**
- `parse_with_spans(src)` — position-tracking S-expression parser that
  returns `(RecExpr, SpanTree)`. The `SpanTree` mirrors the
  `AppliedId` structure of the `RecExpr`: one child per AppliedId
  field. Slots and operator keywords don't get their own children
  because they don't become e-classes.
- `add_expr_with_spans(eg, expr, spans)` — recursive insertion that
  walks `expr` and `spans` in lockstep. Inserts each sub-expression
  into the e-graph, then calls `analysis_data_mut(id).insert(span)`
  to attach the source range to its e-class.
- `semantic_search_with_spans(eg, pattern)` — runs `semantic_search`
  and looks up each result's analysis data to produce
  `SearchResultWithSpans { ..., spans }`.

**The elegant property.** When commutativity merges `(x+y)` and `(y+x)`,
the merged e-class automatically carries spans from both original source
positions. No manual bookkeeping. The `search_spans_after_rewrite_merge`
test demonstrates this directly.

---

### `stress.rs` — growth & timing measurements

A suite of `#[ignore]`-marked benchmarks that probe how the e-graph
scales with input size and rule complexity. Run with:

```
cargo test --release --test entry c_subset::stress -- --ignored --nocapture
```

Each test runs on a worker thread with a 64 MB stack via
`run_with_big_stack` so that deep recursive inputs don't blow the
default 8 MB Rust stack.

**Builders.**
- `linear_add_chain(N)` — left-nested `(((1+2)+3)+...+N)`. Depth = N,
  exercises recursive-walk depth.
- `balanced_add_tree(N)` — same N leaves, depth `O(log N)`. Practical
  at much higher N.
- `n_increments(N)` — function body with N `x = x+1` statements.

**Test families.**
- *Pure insertion* (no rewrites): linear in N, ~5 µs/node in release.
- *Insertion with `ConstFold`*: collapses the chain to a single literal.
- *Saturation with `add-comm` only*: near-linear; comm just registers
  symmetries.
- *Saturation with `add-assoc1/2`*: Catalan-like growth in
  bracketings.
- *Saturation with `comm + assoc`*: the explosive case.
- *Saturation with `comm + assoc + ConstFold`*: ConstFold compresses
  steady state but doesn't bound iteration peak.
- *Function-body increments + context rewrites*: collapses `N` adds
  to a single literal.
- *Span overhead*: compares `EGraph<CSubset>` vs
  `EGraph<CSubset, SpanAnalysis>` insertion times.

**Trajectory tests** (`stress_growth_trajectory_*`). For a fixed input
and very generous limits, prints per-iteration node counts, deltas, and
cumulative wall-clock time. Also reports milestone times (e.g. "first
iteration to reach ≥100K nodes"). Lets you directly answer "how long
to reach a graph of size X?" by reading down the table.

**Stack-size note.** Linear-chain inputs of depth N force three nested
recursive walks (parser + RecExpr-builder + `add_expr`), each at depth
N. Default Rust stack overflows around N=500–1000; the 64 MB stack
raises this to ~10 000. Balanced-tree inputs only need O(log N) stack
and are practical at 100 000+.

---

## Findings from stress testing

### ConstFold doesn't bound the iteration peak

Comm + assoc + ConstFold on a 12-leaf linear chain runs the process
out of memory. The reason isn't that ConstFold fails to fold —
it does. The reason is the *order of operations within an iteration*:

1. **Apply rewrites first.** Comm produces a swapped variant for every
   `add` enode; assoc produces re-associations for every adjacent pair.
   The e-graph balloons in this step.
2. **Then rebuild + process the modify queue.** ConstFold's `modify`
   adds literal nodes and unifies, collapsing the just-created
   variants.

The runner only checks limits between iterations, so the *peak* during
an iteration can be far above any node-count limit. For the 12-leaf
chain, that single-iteration peak exceeds available RAM.

**Implication for the C tool.** Analyses compress steady-state size
but don't tame transient-iteration cost. Rule design has to be
conservative — generic permutation rules over arbitrary `add` nodes
are not safe for large inputs regardless of downstream folding. The
real-world rule set will need pattern guards or scope restrictions.

### Recursion depth is the practical input ceiling

Pure insertion is linear in nodes (~5 µs/node) and well-behaved up to
the recursion limit. The bottleneck on big single-expression inputs
isn't the e-graph — it's the parser/builder/inserter call depth.
Realistic C ASTs are nowhere near this deep, so this is not a concern
for the production tool, but it's a knob to remember when synthesizing
stress inputs.

### Determinism

Repeated runs of the same stress test produce identical metrics —
node counts, class counts, slot counts, and even iteration-by-iteration
trajectories. The slotted e-graph is a deterministic data structure
under our usage.

---

## Test coverage

Total: **70+ tests** in `tests/c_subset/` (the full test suite has
136 passing, all stress tests ignored unless explicitly requested).

| Module | Tests | What they cover |
|---|---|---|
| `tst.rs` | 13 | Algebraic / control-flow equivalences via rewrites |
| `egg_ports.rs` | 6 | Ports of the original egg-based prototype tests |
| `multi_bind.rs` | 4 | `MultiBind<T>` binding semantics |
| `c_loop.rs` | 5 | Loop alpha-equivalence + multi-variable mutation |
| `const_fold.rs` | 7 | Integer / boolean / mixed folding |
| `search.rs` | 11 | E-class-level search, including the "increment in function" demo |
| `spans.rs` | 12 | Span tracking, parsing, merge propagation, headline demo |
| `context_rewrites.rs` | 10 | Guarded sequence-spanning rewrites + composition |
| `stress.rs` | 11 (ignored) | Growth, timing, trajectories — run on demand |

The headline tests:

- **`search_finds_increment_in_function`** — given `int main() { int x = 0; x = x + 1; return x; }`, query `(assign $x (add (var $x) 1))` finds the increment as a sub-expression.
- **`search_returns_source_positions`** — proves that even when 3 source-level occurrences collapse into 2 e-classes, the spans recover all 3 positions.
- **`search_spans_after_rewrite_merge`** — proves that commutativity merging `(x+y)` with `(y+x)` correctly unions their source positions in the merged e-class.

---

## What's deliberately out of scope

- Multi-file source (single `Span` has no file id).
- Incremental updates on file changes (no "remove span" support).
- Tree-sitter integration (the `parse_with_spans` is a stand-in).
- Rewrites that depend on context across larger program fragments (planned but not designed).

Each of these has a notes-and-rationale entry in the project backlog.
