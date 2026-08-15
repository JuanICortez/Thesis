//! `int a, b = 2;` → two declarations.

use crate::cast::TranslationUnit;

/// Splits a multi-declarator declaration into one statement per declarator.
///
/// **Soundness:** unconditional. The declarators in a single declaration are
/// independent, and stage 1 already emits one `StmtKind::Declaration` per
/// declarator — this only has to lift them into separate `Statement`s. The one
/// thing to preserve is order, since a later declarator may refer to an
/// earlier one: `int a = 1, b = a + 1;`.
///
/// TODO: not implemented — see task #4.
pub(super) fn split_declarations(unit: TranslationUnit) -> TranslationUnit {
    unit
}
