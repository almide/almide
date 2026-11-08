import sys
P={
 'none':[],
 'mapentries':[('crates/almide-mir/src/native_map_entries.rs','let found = entries(func);','let found = { let _ = entries(func); Entries::default() };')],
 'wholestr':[('crates/almide-mir/src/render_native_addr_alloc.rs','    let Some(n) = st.consts.get(len).and_then(|n| usize::try_from(*n).ok()) else {','    let Some(n) = st.consts.get(len).and_then(|n| usize::try_from(*n).ok()).filter(|_| crate::native_map_entries::defer_pair(*dst)) else {')],
 'heaps':[('crates/almide-mir/src/render_native_env_types.rs','__AlmdHeaps::Empty => __AlmdHeaps::One(k, v),','__AlmdHeaps::Empty => { let mut h = Vec::new(); h.resize_with(self.words.len().max(k + 1), || None); h[k] = Some(v); __AlmdHeaps::Many(h) }')],
 'placeholder':[('crates/almide-mir/src/render_native_cell_slots.rs','writeln!(out, "// {}: the payload the read before took", var(*d))','writeln!(out, "let mut {}: {} = Default::default();", var(*d), rec::owned_ty(payload))')],
}
for f,o,n in P[sys.argv[1]]:
    s=open(f).read(); assert s.count(o)==1,(f,o); open(f,'w').write(s.replace(o,n))
print('applied',sys.argv[1])
