# Implementation Plan — Library vs. Tool Split

This document organizes the work that has been prototyped in
`tests/c_subset/` and decides which parts should be **upstreamed into the
`slotted-egraphs` library** versus **built as a consumer tool** that uses
`slotted-egraphs` as a dependency.

The guiding principle: anything that is **language-agnostic infrastructure**
belongs in the library; anything that is **specific to "C semantic search"**
belongs in the consumer tool.

---

## Part A — What should go into `slotted-egraphs` (library)

### A.1 — `MultiBind<T>` upstream

**What.** Promote `tests/c_subset/multi_bind.rs` into `src/lang.rs` next
to the existing `Bind<T>`.

**Why upstream.** `Bind<T>` already lives in the library and binds a
single slot. `MultiBind<T>` is the natural multi-slot generalization:
useful for *any* language with multi-binder constructs — multi-argument
lambdas, let-rec groups, hoisted-locals function bodies, etc. There's
nothing C-specific about it.

**How.** Move the struct definition and `LanguageChildren` impl into the
library. The `define_language!` macro already accepts user-defined
`LanguageChildren` types as fields, so no derive-macro changes are
needed.

**Estimated effort.** Small. ~80 lines of code + a couple of upstream
unit tests.

---

### A.2 — Generic `semantic_search` API

**What.** Promote `semantic_search` from `tests/c_subset/search.rs` into
the library, e.g. as part of the `rewrite/` module or a new `search/`
module. **Bundle a small ematch helper** (`ematch_all_with_roots`) that
exposes the matched e-class id, eliminating the `pattern_root_eclass`
workaround entirely.

**Why upstream.** The implementation is purely generic over `Language`
and `Analysis` — it wraps `ematch_all` with two pieces of bookkeeping
(reconstructing the matched e-class, and extracting a representative).
Both are useful to any consumer doing pattern-based search; both
currently have to be re-implemented per project.

**Why the ematch helper matters.** Inside `src/rewrite/ematch.rs`, the
`ematch_impl` function already knows which e-class it's matching at —
`ematch_all` just discards that information when it converts results
to `Subst`. Our prototype's `pattern_root_eclass` re-derives this by
re-instantiating the pattern via `eg.lookup`, which is:
- Redundant work (the matcher already had the answer).
- Potentially less robust under group-symmetry edge cases.
A 10-line variant of `ematch_all` that returns `Vec<(AppliedId, Subst)>`
removes both problems.

**How.**
- Add `ematch_all_with_roots` next to `ematch_all` in `src/rewrite/ematch.rs`.
  Same loop, just emits the root `AppliedId` alongside each `Subst`.
- Move `SearchResult<L>` and `semantic_search` into a new `src/search/mod.rs`.
  Library users get them via `use slotted_egraphs::semantic_search;`.
- The `pattern_root_eclass` helper from the prototype goes away — it's
  no longer needed.

**Estimated effort.** Small. ~70 lines (~10 for the ematch helper,
~50 for `semantic_search` proper, ~10 for tests).

---

### A.3 — Position-tracking parser + `SpanTree` + span-aware insertion — *moved to the tool (see B.1.3)*

**Decision (revised).** This item is **no longer planned as library work.**
Spans live in the consumer crate `csearch-core` (`src/span.rs`), not in
`slotted-egraphs`. The generic mechanism that was originally slated for
upstreaming —
- `Span { start, end }`
- `SpanTree { span, children }`
- `parse_with_spans` (the position-tracking S-expression parser)
- `add_expr_with_spans` (span-aware recursive insertion)

— is being built tool-side instead. See **B.1.3** for the current home.

**Why the change.** The original rationale for upstreaming was that
every consumer needs the same source-provenance plumbing. In practice
the tool's position type is already richer than a minimal library `Span`
(multi-file `file_id`, byte range, and eventually row/col — see Part C.1),
so the library `Span` would only ever be a stepping stone the tool
immediately wraps. Rather than maintain a minimal `Span`/`SpanTree` in
the library that no consumer uses directly, we keep the whole mechanism
in `csearch-core` where it can evolve with the tool's needs.

**What stays generic.** `add_expr_with_spans` remains generic over
`L: Language` and `N: Analysis<L>` and uses a callback so it doesn't hard-code
any specific analysis — but it lives in `csearch-core`, not the library. If a
second consumer ever needs the same plumbing, revisit upstreaming then.

**Estimated effort.** Medium. ~150 lines, now tool-side in `csearch-core`.

---

### A.4 — From the existing backlog: `Searcher` / `Applier` traits port

**What.** Port egg's `Searcher` and `Applier` traits as a more
composable alternative to the existing closure-based `RewriteT`. Adds
`ConditionalApplier`, variable-binding validation, introspection.

**Why upstream.** Library-internal architecture; affects every consumer
that writes rewrites with side-conditions. Already on the backlog
(medium priority).

**How.** Add the traits and their default impls; keep `RewriteT` as the
existing closure-based path so we don't break anyone. Provide a bridge
that turns trait impls into `RewriteT`s.

**Estimated effort.** Medium. ~250 lines.

---

### A.5 — From the existing backlog: `Analysis` trait improvements

**What.** Possibly extend the `Analysis` trait with egg-style hooks:
`DidMerge`, `pre_union`, `remake`. Carefully — slotted e-graphs carry
slot bijections through unions, so `pre_union` would need to expose
that information (egg's version doesn't).

**Why upstream.** Library-internal. Useful for analyses that need to
react to union events.

**Caveat.** Needs design work first (low priority on backlog). Should
not be done as a naive port.

---

## Part B — What should be built as a consumer tool

The C semantic-search tool is the planned application. It depends on
`slotted-egraphs` (with the additions from Part A) and a `tree-sitter`
crate. Everything below is **outside the library**.

### B.1 — The `csearch` crate (or whatever it's named)

A new Cargo crate, separate from `slotted-egraphs`, with this rough
structure:

```
csearch/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── lang.rs           ← CSubset (define_language!)
│   ├── rewrite.rs        ← C-specific rewrites
│   ├── const_fold.rs     ← ConstFold analysis
│   ├── span_analysis.rs  ← SpanAnalysis (CSubset-specific)
│   ├── tree_sitter.rs    ← C source → ABT translation
│   ├── lower.rs          ← AST → ABT (slot binding, hoisting locals,
│   │                       loop normalization)
│   └── search.rs         ← high-level query API
└── examples/
    └── grep.rs           ← CLI for "search this pattern in this codebase"
```

#### B.1.1 — Language definition (`lang.rs`)

The `CSubset` enum we already wrote. Stays exactly as it is, just
relocated into the new crate.

#### B.1.2 — Rewrite rules (`rewrite.rs`)

The arithmetic, boolean, and control-flow rewrites we already wrote.
This is where future **context-sensitive** rules will live (program
permutations, init+increment folding, etc.) — none of those are
C-language-agnostic, so they don't belong in the library.

#### B.1.3 — `ConstFold` and `SpanAnalysis` (`const_fold.rs`,
`span_analysis.rs`)

Both analyses are written *over `CSubset`*. They need to know about
`CSubset::Num`, `CSubset::Add`, etc. They live in the consumer crate.

`SpanAnalysis` builds on the crate-local span infrastructure — `Span`,
`SpanTree`, `parse_with_spans`, `add_expr_with_spans` in
`csearch-core/src/span.rs` (formerly slated for the library as Part A.3,
now tool-side). The `Analysis<CSubset>` impl itself stays C-specific.

#### B.1.4 — Tree-sitter integration (`tree_sitter.rs`, `lower.rs`)

Brand-new code, no prototype yet. Two phases:

**Phase 1: parse and build the AST.**
- Use `tree-sitter-c` to parse a `.c` file into a CST.
- Walk the CST, building an internal `CAst` representation that's
  closer to what we need but still has all the C-isms.
- Each AST node carries a Tree-sitter span (`start_byte`, `end_byte`).

**Phase 2: lower AST to ABT.**
- Hoist all local variables to the enclosing function (collect during
  the walk; emit as the `Fun`'s `MultiBind`).
- Normalize loops: `for (init; cond; step) body` → `(seq init (loop cond (seq body step)))`,
  `do body while (cond)` → `(seq body (loop cond body))`.
- Convert `++`/`--`/compound-assigns to plain `assign`.
- Track spans through the lowering: each ABT node carries the span of
  the C source it came from. Output a `(RecExpr<CSubset>, SpanTree)`
  matching the library's expected shape.

#### B.1.5 — High-level query API (`search.rs`)

A user-facing API that wraps the library:

```rust
pub struct CodeBase {
    egraph: EGraph<CSubset, SpanAnalysis>,
    sources: HashMap<FileId, String>,
}

impl CodeBase {
    pub fn add_file(&mut self, path: &Path) -> Result<()>;
    pub fn saturate(&mut self, rules: &[Rewrite<...>]) -> SaturationReport;
    pub fn search(&self, pattern_src: &str) -> Vec<Match>;
}

pub struct Match {
    pub matched_expr: String,    // pretty-printed
    pub locations: Vec<Location>,
}
pub struct Location {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}
```

#### B.1.6 — CLI (`examples/grep.rs`)

A binary that takes a directory + a pattern and prints matches in a
grep-like format:

```
$ csearch ./src 'while ($i < ?n) (assign $i (add (var $i) 1))'
src/lib.rs:42:8  while (i < limit) i = i + 1;
src/util.rs:103:4  while (count < total) count++;
```

---

## Part C — Backlog (mental notes — not yet implemented)

These items are tracked but neither library work nor part of the initial
tool. They influence the design and may inform priority.

1. **Multi-file `Span` support.** The library's `Span` should stay
   minimal (just byte offsets). The consumer crate's `SpanAnalysis`
   carries a richer `(file_id, byte_range)`. This is mostly a tool-side
   problem, but worth keeping in mind when shaping Part A.3.

2. **Incremental updates.** When a file changes, we want to invalidate
   only the affected e-classes, not rebuild from scratch. Requires:
   - A reverse index `FileId → Set<Id>` of e-classes touched by a file.
   - A `remove_span(eg, id, span)` operation. The current `Analysis`
     trait is append-only on merge; a "remove" hook is non-trivial.
   - Potentially per-file e-graphs federated by a higher-level layer.

3. **Context-sensitive rewrites.** Rules that match larger fragments to
   stay sound under our program-as-is mental model. Example: only fold
   `x = 0; x = x + 1` to `x = 1` if both assignments are syntactically
   adjacent in a `seq`. Library doesn't need to know about this — it's
   pattern syntax + careful design at the consumer level.

4. **Program permutations.** Rewrites that commute independent
   statements, reorder safe operations, etc. Will need a side-effect
   analysis to determine "independence". Tool-side.

5. **`search_multiple_matches` semantics.** Already resolved in the
   prototype: `semantic_search` returns *distinct e-classes*; spans
   recover *source-level occurrences*. Documented; no further work
   needed unless we discover a third notion of search we want.

6. **`occurrence_search`.** Originally planned as a separate API for
   counting source occurrences. Superseded by spans (which give
   strictly more information). Could still be added later if we want
   occurrence counts on synthesized expressions that have no source
   position.

7. **Tuples / product types as `LanguageChildren`.** Was needed under
   the previous "loops as recursive functions" design. With the current
   direct `Loop(cond, body)` shape, multi-variable mutation works
   without tuples. Still potentially useful for other languages but
   not blocking our tool.

8. **Library `Analysis` trait improvements** (egg-style). Listed in
   Part A.5 as a possible upstream — but low priority. Not needed for
   the initial tool.

---

## Suggested order of work

1. **Library upstream — A.1 (`MultiBind`).** Smallest unit; clean
   first commit; gets used immediately by the tool.
2. **Library upstream — A.2 (`semantic_search`).** Enables the tool's
   query API to depend on it directly instead of vendoring.
3. **Tool — spans (`Span` / `SpanTree` / `parse_with_spans` / `add_expr_with_spans`).**
   The big infrastructure piece, now built tool-side in
   `csearch-core/src/span.rs` (formerly Part A.3). Underpins the tool's
   `SpanAnalysis`.
4. **Tool — B.1.1, B.1.2, B.1.3.** Move `CSubset`, rewrites, analyses
   into the new crate, depending on the freshly-upstreamed library
   helpers.
5. **Tool — B.1.4.** Tree-sitter integration and lowering. The bulk
   of the new engineering effort.
6. **Tool — B.1.5, B.1.6.** Query API and CLI.
7. **Library upstream — A.4 (`Searcher`/`Applier`).** Once we have a
   real consumer, we'll know better which trait shape they want.
8. **Library upstream — A.5 (analysis trait improvements).** Defer
   until concrete need from the tool.

Roughly: ~3 weeks of library work, then ~6-8 weeks for the tool MVP,
assuming Tree-sitter integration is the heaviest lift.
