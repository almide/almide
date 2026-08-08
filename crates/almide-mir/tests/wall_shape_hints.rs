//! The WallShape hint matrix gate (#931).
//!
//! Every KNOWN wall shape must carry BOTH texts — the plain-language headline
//! and the documented rewrite hint — or the CLI's shape-aware rendering
//! silently degrades to the reason-as-headline form for that shape. `Other`
//! must carry NEITHER: it is the "no specific rewrite" bucket, and giving it
//! texts would stamp a generic hint onto walls it does not fit. Point-wise
//! drift (a new shape added without its texts) fails here, not in the field.

use almide_mir::lower::WallShape;

const KNOWN_SHAPES: &[WallShape] = &[
    WallShape::WhileHeapAccumulator,
    WallShape::HeapResultBind,
    WallShape::VariantValueMatch,
    WallShape::CallArgument,
    WallShape::TailExtraction,
];

#[test]
fn every_known_shape_carries_headline_and_rewrite_hint() {
    for shape in KNOWN_SHAPES {
        assert!(
            shape.headline().is_some(),
            "{shape:?} has no headline — the CLI would fall back to the raw reason"
        );
        assert!(
            shape.rewrite_hint().is_some(),
            "{shape:?} has no rewrite hint — the wall would offer no way forward"
        );
    }
}

#[test]
fn other_carries_neither_text() {
    assert_eq!(WallShape::Other.headline(), None);
    assert_eq!(WallShape::Other.rewrite_hint(), None);
}

#[test]
fn shape_survives_construction_and_nesting() {
    let span = almide_ir::Span {
        line: 3,
        col: 5,
        end_col: 8,
    };
    let inner = almide_mir::lower::LowerError::shaped(
        Some(span),
        WallShape::WhileHeapAccumulator,
        "while body with a heap-accumulator reassignment",
    );
    // The wrapper pattern pipeline_c uses: nesting adds context to the reason
    // but preserves the inner wall's span AND shape.
    let wrapped = almide_mir::lower::LowerError::shaped(
        inner.span(),
        inner.shape(),
        format!("main is outside the MIR-lowering subset: {inner}"),
    );
    assert_eq!(wrapped.span(), Some(span));
    assert_eq!(wrapped.shape(), WallShape::WhileHeapAccumulator);
    // `at` (no known shape) and the spanless legacy form both read as Other.
    let at = almide_mir::lower::LowerError::at(Some(span), "reason");
    assert_eq!(at.shape(), WallShape::Other);
    assert_eq!(at.span(), Some(span));
    let spanless = almide_mir::lower::LowerError::at(None, "reason");
    assert_eq!(spanless.shape(), WallShape::Other);
    assert_eq!(spanless.span(), None);
}
