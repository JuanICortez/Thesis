//! `a += b` → `a = a + b`.

use crate::cast::TranslationUnit;

/// Rewrites compound assignment into plain assignment.
///
/// **Soundness trap: this duplicates the LHS.** `arr[i++] += 1` evaluates the
/// index once in C and twice after a naive desugaring, which is a different
/// program. Restrict the pass to side-effect-free lvalues and leave everything
/// else as it is — declining to rewrite is always sound, rewriting wrongly is
/// not.
///
/// Blocked on stage 1: compound assignment currently lowers to `Unsupported`
/// (deliberately — modelling `a += b` as `a = b` was a real bug), so there is
/// nothing here to rewrite until `StmtKind` carries the operator.
///
/// TODO: not implemented — see task #5.
pub(super) fn desugar_compound_assign(unit: TranslationUnit) -> TranslationUnit {
    unit
}
