import sys
P={
 'none':[],
 'keyed':[('crates/almide-mir/src/render_native_keyed.rs','.filter(|(_, set)| !set.is_empty());','.filter(|_| false);')],
 'env':[('crates/almide-mir/src/render_native_env_types.rs','if !WIDE_CLOSURES.with(|w| w.get()) && CAPTURE_KINDS','if false && CAPTURE_KINDS'),
        ('crates/almide-mir/src/render_native_env_types.rs','    if words.len() > 4 {\n','    if true {\n')],
 'view':[('crates/almide-mir/src/render_native_carrier.rs','            NTy::Vec => Some((NTy::VecRef, "&[i64]".to_string(), "&v[..]")),\n','')],
 'take':[('crates/almide-mir/src/render_native_cell_slots.rs','rec::has_default(payload) && crate::render_native_keyed::cell_read_takes','false && crate::render_native_keyed::cell_read_takes')],
 'rescopy':[('crates/almide-mir/src/render_native_loop.rs','            if args.len() == 1\n                && st.tys.get(&args[0]).is_some_and(|t| crate::render_native_move::last_use','            if false && args.len() == 1\n                && st.tys.get(&args[0]).is_some_and(|t| crate::render_native_move::last_use')],

 'refdup':[('crates/almide-mir/src/render_native_loop.rs','if crate::render_native_borrow_read::ref_dup(*dst)','if false && crate::render_native_borrow_read::ref_dup(*dst)')],
 'guard':[('crates/almide-mir/src/render_native_cell_slots.rs','if crate::render_native_keyed::cell_read_borrows(','if false && crate::render_native_keyed::cell_read_borrows(')],
 'storemove':[('crates/almide-mir/src/render_native_keyed.rs','Some(crate::render_native_move::last_use(func, at, src, t, &c.arms))','Some(false && crate::render_native_move::last_use(func, at, src, t, &c.arms))')],
 'elemshare':[('crates/almide-mir/src/render_native_addr_op.rs','                note_field(*d, ad.base, format!("{}[{ix}]", var(ad.base)));\n','')],
 'pairs':[('crates/almide-mir/src/native_floor_tail.rs','= (got, dying_here, pair) {','= (got, false, pair) {')],
}
for f,o,n in P[sys.argv[1]]:
    s=open(f).read(); assert s.count(o)==1,(f,o); open(f,'w').write(s.replace(o,n))
print('applied',sys.argv[1])
