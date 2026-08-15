//! `for (init; cond; step) body` → `init; while (cond) { body; step }`.

use crate::cast::TranslationUnit;

/// Rewrites `for` loops into `while` loops so stage 3 sees one loop form.
///
/// **Soundness trap: this breaks `continue`.** In a real `for`, `continue`
/// still runs the step expression before re-testing the condition; in the
/// naive `while` translation it jumps straight to the condition, so a loop
/// that terminated now runs forever. Guard the pass on "body contains no
/// `continue`", or hoist the step into a `continue`-aware form before
/// rewriting.
///
/// A second, quieter one: an omitted condition (`for (;;)`) means *true*, not
/// *false*. Dropping a missing condition would invert the loop.
///
/// TODO: not implemented — see task #7.
pub(super) fn for_to_while(unit: TranslationUnit) -> TranslationUnit {
    unit
}
