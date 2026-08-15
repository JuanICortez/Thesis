// The CST → CAst → e-graph pipeline is one module per stage, in this order:
//
//   lower      stage 1: CST  → CAst   (total; never drops, never rewrites)
//   normalize  stage 2: CAst → CAst   (semantics-preserving passes)
//   abt        stage 3: CAst → e-graph (the only lossy stage)
//
// `cast` is not a stage: it holds the tree the stages pass between, plus the
// two ways to render it. Nothing in it rewrites.
pub mod abt;
pub mod cast;
pub mod lower;
pub mod normalize;

pub mod analysis;
pub mod codebase;
pub mod context_rewrites;
pub mod lang;
pub mod pattern;
pub mod rewrite;
pub mod span;
pub mod tree_sitter;
