//! `i++` → `i = i + 1`.

use crate::cast::TranslationUnit;

/// Rewrites increment and decrement into assignment.
///
/// **Soundness trap 1: LHS duplication**, exactly as in
/// [`desugar_compound_assign`](super::desugar_compound_assign). Restrict to
/// side-effect-free lvalues.
///
/// **Soundness trap 2: value position.** `i = j++` is *not* `i = j = j + 1` —
/// the postfix form yields the old value, the prefix form the new one. In
/// statement position the value is discarded and the two agree, so that is the
/// safe subset to implement first; increments nested inside an expression need
/// a temporary and should stay untouched until then.
///
/// TODO: not implemented — see task #6.
pub(super) fn desugar_incdec(unit: TranslationUnit) -> TranslationUnit {
    unit
}
