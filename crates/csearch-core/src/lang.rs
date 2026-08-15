// CSubset language definition (define_language!)
use slotted_egraphs::{define_language, AppliedId, Language, LanguageChildren, Slot, SyntaxElem};

define_language!(
    pub enum CSubset {
        Var(Slot) = "var",
        Num(i64),

        Add(AppliedId, AppliedId) = "add",
        Sub(AppliedId, AppliedId) = "sub",
        Mul(AppliedId, AppliedId) = "mul",
        Div(AppliedId, AppliedId) = "div",

        Eq(AppliedId, AppliedId) = "eq",
    }
);
