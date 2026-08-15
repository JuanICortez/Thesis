//! Stage 2: CAst → CAst, semantics-preserving.
//!
//! Stage 1 models C as it is written; stage 3 needs a small, regular subset.
//! This is where the gap is closed, one pass at a time, so that each rewrite
//! is separately testable and separately disableable.
//!
//! One pass per file. This module owns only the contract, the [`PASSES`] table
//! and the properties every pass must satisfy.
//!
//! **Contract, for every pass:**
//!
//! 1. *Semantics-preserving* — the only property no test here can check.
//!    It has to be argued in the pass's own doc comment, which is why each
//!    stub carries its soundness trap rather than a bare `TODO`.
//! 2. *Idempotent* — `p(p(x)) == p(x)`. Passes run in a fixed order but a
//!    later pass can reintroduce a shape an earlier one handled, so this is
//!    what makes the pipeline safe to re-run.
//! 3. *Never introduces `Unsupported`* — a pass may only ever narrow the set
//!    of constructs. Turning a modelled construct into an unmodelled one would
//!    lose coverage silently.
//!
//! Properties 2 and 3 are checked automatically for every entry in [`PASSES`],
//! so adding a pass to that table is what enrolls it. Nothing is implemented
//! yet: every pass is the identity, which satisfies both properties vacuously.
//! **The contract tests have no teeth until the bodies are real** — see
//! `no_pass_is_implemented_yet`, which exists to say so out loud.

mod desugar_compound_assign;
mod desugar_incdec;
mod flatten_blocks;
mod for_to_while;
mod split_declarations;

#[cfg(test)]
mod testing;

use crate::cast::TranslationUnit;

/// A stage-2 pass. By value rather than `&mut`, so a pass that restructures a
/// statement list can build a new one without fighting the borrow checker.
pub type Pass = fn(TranslationUnit) -> TranslationUnit;

/// Every pass, in the order [`normalize`] applies them, paired with a name for
/// test output.
///
/// Order matters: `split_declarations` runs first so later passes only ever
/// see one declarator per statement, and `flatten_blocks` runs last so it can
/// collapse the blocks `for_to_while` introduces.
pub const PASSES: &[(&str, Pass)] = &[
    ("split_declarations", split_declarations::split_declarations),
    (
        "desugar_compound_assign",
        desugar_compound_assign::desugar_compound_assign,
    ),
    ("desugar_incdec", desugar_incdec::desugar_incdec),
    ("for_to_while", for_to_while::for_to_while),
    ("flatten_blocks", flatten_blocks::flatten_blocks),
];

/// Applies every pass in [`PASSES`], in order.
///
/// Currently the identity, since no pass is implemented.
pub fn normalize(unit: TranslationUnit) -> TranslationUnit {
    PASSES.iter().fold(unit, |unit, (_, pass)| pass(unit))
}

#[cfg(test)]
mod tests {
    use super::testing::{lower, CORPUS};
    use super::*;
    use crate::cast::{collect_unsupported, dump};
    use std::collections::HashMap;

    fn unsupported_counts(unit: &TranslationUnit) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for found in collect_unsupported(unit) {
            *counts.entry(found.kind).or_insert(0) += 1;
        }
        counts
    }

    /// Contract 2. Compared through `dump`, which ignores spans — a pass that
    /// rebuilds a statement will not reproduce the original byte offsets.
    #[test]
    fn every_pass_is_idempotent() {
        for (name, pass) in PASSES {
            for src in CORPUS {
                let once = pass(lower(src));
                let twice = pass(once.clone());
                assert_eq!(
                    dump(&once),
                    dump(&twice),
                    "pass `{name}` is not idempotent on:\n{src}"
                );
            }
        }
    }

    /// Contract 3. A pass may narrow the set of unmodelled constructs — that
    /// is the point of desugaring — but never widen it.
    #[test]
    fn no_pass_introduces_unsupported() {
        for (name, pass) in PASSES {
            for src in CORPUS {
                let before = lower(src);
                let before_counts = unsupported_counts(&before);
                let after_counts = unsupported_counts(&pass(before));

                for (kind, after) in &after_counts {
                    let before = before_counts.get(kind).copied().unwrap_or(0);
                    assert!(
                        *after <= before,
                        "pass `{name}` introduced {} extra `{kind}` node(s) on:\n{src}",
                        after - before,
                    );
                }
            }
        }
    }

    /// The pipeline as a whole, not just each pass — running `normalize` twice
    /// must be the same as running it once, or callers have to care how many
    /// times it has been applied.
    #[test]
    fn normalize_is_idempotent() {
        for src in CORPUS {
            let once = normalize(lower(src));
            let twice = normalize(once.clone());
            assert_eq!(
                dump(&once),
                dump(&twice),
                "normalize is not idempotent on:\n{src}"
            );
        }
    }

    /// Guards the vacuity admitted in the module docs: while every pass is the
    /// identity, the contract tests prove nothing. This fails the moment a
    /// pass does real work — at which point delete it, because the contract
    /// tests above have become meaningful on their own.
    #[test]
    fn no_pass_is_implemented_yet() {
        for (name, pass) in PASSES {
            for src in CORPUS {
                let unit = lower(src);
                assert_eq!(
                    dump(&unit),
                    dump(&pass(unit.clone())),
                    "pass `{name}` now rewrites something — delete this test; \
                     the contract tests above are no longer vacuous"
                );
            }
        }
    }
}
