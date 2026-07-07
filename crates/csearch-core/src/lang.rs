// CSubset language definition (define_language!)
use slotted_egraphs::{define_language, AppliedId, Language, LanguageChildren, Slot, SyntaxElem};

define_language!(
    pub enum CSubset {
        Var(Slot) = "var",
        Num(i32),

        Add(AppliedId, AppliedId) = "add",
        Sub(AppliedId, AppliedId) = "sub",
        Mul(AppliedId, AppliedId) = "mul",

        Eq(AppliedId, AppliedId) = "eq",
    }
);
