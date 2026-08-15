use slotted_egraphs::{Analysis, EGraph, Id, SmallHashSet};

use super::span::Span;
use crate::lang::CSubset;

// #[derive(Default, Clone)]
// struct ConstFolding;
// #[derive(Default, Clone)]
// struct SpanAnalysis;

// Combination of both Analysis
#[derive(Default, Clone)]
pub struct CAnalysis;

impl Analysis<CSubset> for CAnalysis {
    type Data = (Option<i64>, SmallHashSet<Span>);

    fn make(eg: &EGraph<CSubset, Self>, enode: &CSubset) -> Self::Data {
        let apply_bin_op = |lhs_id: Id, rhs_id: Id, op: fn(i64, i64) -> Option<i64>| {
            let lhs_val = eg.analysis_data(lhs_id).0;
            let rhs_val = eg.analysis_data(rhs_id).0;

            lhs_val.zip(rhs_val).and_then(|(x, y)| op(x, y))
        };

        let const_data = match enode {
            CSubset::Num(n) => Some(*n),
            CSubset::Add(lhs, rhs) => apply_bin_op(lhs.id, rhs.id, i64::checked_add),
            CSubset::Sub(lhs, rhs) => apply_bin_op(lhs.id, rhs.id, i64::checked_sub),
            CSubset::Mul(lhs, rhs) => apply_bin_op(lhs.id, rhs.id, i64::checked_mul),
            // `checked_div` also covers `i64::MIN / -1`, which overflows.
            CSubset::Div(lhs, rhs) => apply_bin_op(lhs.id, rhs.id, i64::checked_div),
            _ => None,
        };

        let span_data = SmallHashSet::default();
        (const_data, span_data)
    }

    fn merge(lnode: Self::Data, rnode: Self::Data) -> Self::Data {
        let const_value = match (lnode.0, rnode.0) {
            (Some(x), Some(y)) => {
                assert_eq!(x, y);
                Some(x)
            }
            (Some(x), _) => Some(x),
            (_, Some(y)) => Some(y),
            (_, _) => None,
        };

        let mut span_data = lnode.1;
        span_data.extend(rnode.1);

        (const_value, span_data)
    }

    fn modify(eg: &mut EGraph<CSubset, Self>, id: Id) {
        let value = eg.analysis_data(id).0;
        if let Some(n) = value {
            let added = eg.add(CSubset::Num(n));
            let this = eg.mk_identity_applied_id(id);
            eg.union(&added, &this);
        }
    }
}
