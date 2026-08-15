//! Collapses redundant nested compounds.

use crate::cast::TranslationUnit;

/// Merges a nested block into its parent when doing so cannot change meaning.
///
/// **Soundness trap: shadowing.** Two sibling blocks that each declare `x` are
/// declaring two different variables; flattening them into one block merges
/// those into a single `x`, and an inner block that shadows an outer name
/// changes which declaration a later reference resolves to. The pass must
/// decline to merge blocks whose declared names collide with the names already
/// live in the parent.
///
/// Runs last, so it can also collapse the blocks
/// [`for_to_while`](super::for_to_while) introduces.
///
/// TODO: not implemented — see task #8.
pub(super) fn flatten_blocks(unit: TranslationUnit) -> TranslationUnit {
    unit
}
