// A self-host link can add declared types before its lowering refuses.
// Restoring only RECS leaves INPUTS claiming those types were installed,
// so a later link requesting the same type would skip rebuilding it.
pub(crate) struct LinkCheckpoint {
    recs: Vec<RecDecl>,
    input_types: Option<usize>,
    scalar_instances: std::collections::BTreeSet<String>,
}

pub(crate) fn link_checkpoint() -> LinkCheckpoint {
    LinkCheckpoint {
        recs: snapshot(),
        // extend_with only appends to the type vector; layouts are immutable.
        input_types: INPUTS.with(|i| i.borrow().as_ref().map(|(_, _, ts)| ts.len())),
        scalar_instances: SCALAR_INSTANCES.with(|s| s.borrow().clone()),
    }
}

pub(crate) fn restore_link_checkpoint(saved: LinkCheckpoint) {
    restore(saved.recs);
    INPUTS.with(|i| {
        let mut i = i.borrow_mut();
        match saved.input_types {
            Some(n) => i
                .as_mut()
                .expect("linking preserves installed inputs")
                .2
                .truncate(n),
            None => *i = None,
        }
    });
    SCALAR_INSTANCES.with(|s| *s.borrow_mut() = saved.scalar_instances);
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    #[test]
    fn refused_link_does_not_hide_types_from_a_later_link() {
        use almide_lang::types::Ty;
        let a = Ty::Tuple(vec![Ty::String, Ty::Int].into());
        let b = Ty::Tuple(vec![Ty::String, Ty::Bool].into());
        let _guard = install_derived(
            &Default::default(),
            &Default::default(),
            vec![a.clone()],
            Default::default(),
        );
        let first = rec_index(&block_key(&a)).unwrap();
        let checkpoint = link_checkpoint();
        extend_with(std::slice::from_ref(&b));
        assert!(rec_index(&block_key(&b)).is_some());
        restore_link_checkpoint(checkpoint);
        assert_eq!(rec_index(&block_key(&a)), Some(first));
        assert!(rec_index(&block_key(&b)).is_none());
        extend_with(std::slice::from_ref(&b));
        assert!(rec_index(&block_key(&b)).is_some());
        assert!(rec_index(&block_key(&a)).is_some());
    }
}
