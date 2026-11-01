// `include!`d part of wasi_p3.rs (#4306, wasi_p3_net.rs): the net shim's text and address helpers.

fn net_m8(off: u64) -> MemArg {
    MemArg { offset: off, align: 0, memory_index: 0 }
}
fn net_m16(off: u64) -> MemArg {
    MemArg { offset: off, align: 1, memory_index: 0 }
}
fn net_m32(off: u64) -> MemArg {
    MemArg { offset: off, align: 2, memory_index: 0 }
}

type Sink<'a> = wasm_encoder::InstructionSink<'a>;

/// `w = w + byte`.
fn net_put_byte(i: &mut Sink<'_>, w: u32, byte: u8) {
    i.local_get(w).i32_const(byte as i32).i32_store8(net_m8(0));
    i.local_get(w).i32_const(1).i32_add().local_set(w);
}

/// `w = w + piece`.
fn net_put(i: &mut Sink<'_>, w: u32, p: NetPiece) {
    if p.len == 0 {
        return;
    }
    i.local_get(w).i32_const(p.at).i32_const(p.len).memory_copy(0, 0);
    i.local_get(w).i32_const(p.len).i32_add().local_set(w);
}

/// `(dst, src, len) -> dst_end`: `src[..len]` escaped as
/// `almide_rt_errtext_escape` escapes it (C-418): `\n` `\r` `\t` `\0`, every
/// other character of the rule's table as `\u{<lowercase hex>}`, every other
/// byte as is — the quote and the backslash included (http's `$http_quote`
/// escapes those too; the net heads do not).
fn net_fn_esc() -> Function {
    let (dst, src, len) = (0u32, 1u32, 2u32);
    let (k, b, cp, m, sh) = (3u32, 4u32, 5u32, 6u32, 7u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(src).local_get(k).i32_add().i32_load8_u(net_m8(0)).local_set(b);
    i.block(BlockType::Empty);
    for (byte, esc) in [(b'\t', b't'), (b'\r', b'r'), (b'\n', b'n'), (0u8, b'0')] {
        i.local_get(b).i32_const(byte as i32).i32_eq().if_(BlockType::Empty);
        net_put_byte(&mut i, dst, b'\\');
        net_put_byte(&mut i, dst, esc);
        i.br(1);
        i.end();
    }
    i.local_get(b).i32_const(0x20).i32_lt_u();
    i.local_get(b).i32_const(0x7f).i32_eq();
    i.i32_or().if_(BlockType::Empty);
    for c in *b"\\u{" {
        net_put_byte(&mut i, dst, c);
    }
    i.local_get(b).i32_const(16).i32_ge_u().if_(BlockType::Empty);
    net_hex_digit(&mut i, b, 4, dst);
    i.end();
    net_hex_digit(&mut i, b, 0, dst);
    net_put_byte(&mut i, dst, b'}');
    i.br(1);
    i.end();
    let addr = |i: &mut Sink<'_>| {
        i.local_get(src).local_get(k).i32_add();
    };
    almide_wasm::emit_errtext_multibyte(&mut i, &addr, (k, len, b, dst), (cp, m, sh), 1);
    i.local_get(dst).local_get(b).i32_store8(net_m8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(dst);
    i.end();
    f
}

/// Write the lowercase hex digit of `(v >> shift) & 15` at `w`, `w += 1`.
fn net_hex_digit(i: &mut Sink<'_>, v: u32, shift: i32, w: u32) {
    i.local_get(w);
    i.local_get(v).i32_const(shift).i32_shr_u().i32_const(15).i32_and();
    i.local_get(v).i32_const(shift).i32_shr_u().i32_const(15).i32_and().i32_const(10).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32)).i32_const('0' as i32).else_().i32_const('a' as i32 - 10).end();
    i.i32_add().i32_store8(net_m8(0));
    i.local_get(w).i32_const(1).i32_add().local_set(w);
}

/// `(dst, v) -> dst_end`: the signed decimal of `v`, as Rust's `{}` writes
/// an i64 (`-` then the magnitude; i64::MIN's magnitude read unsigned).
fn net_fn_dec() -> Function {
    let (dst, v) = (0u32, 1u32);
    let (n, j) = (2u32, 3u32);
    let (u, t) = (4u32, 5u32);
    let mut f = Function::new([(2, ValType::I32), (2, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(v).local_set(u);
    i.local_get(v).i64_const(0).i64_lt_s().if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b'-');
    i.i64_const(0).local_get(v).i64_sub().local_set(u);
    i.end();
    i.i32_const(1).local_set(n);
    i.local_get(u).local_set(t);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(t).i64_const(10).i64_lt_u().br_if(1);
    i.local_get(t).i64_const(10).i64_div_u().local_set(t);
    i.local_get(n).i32_const(1).i32_add().local_set(n);
    i.br(0).end().end();
    i.local_get(n).local_set(j);
    i.local_get(u).local_set(t);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_eqz().br_if(1);
    i.local_get(j).i32_const(1).i32_sub().local_set(j);
    i.local_get(dst).local_get(j).i32_add();
    i.local_get(t).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const('0' as i32).i32_add();
    i.i32_store8(net_m8(0));
    i.local_get(t).i64_const(10).i64_div_u().local_set(t);
    i.br(0).end().end();
    i.local_get(dst).local_get(n).i32_add();
    i.end();
    f
}

/// `(ptr, len) -> v`: the decimal net_wasm.almd writes (`int.to_string`:
/// an optional `-`, then digits).
fn net_fn_int() -> Function {
    let (ptr, len) = (0u32, 1u32);
    let (k, neg) = (2u32, 3u32);
    let v = 4u32;
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(len).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.local_get(ptr).i32_load8_u(net_m8(0)).i32_const('-' as i32).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(neg).i32_const(1).local_set(k);
    i.end();
    i.end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(v).i64_const(10).i64_mul();
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('0' as i32).i32_sub().i64_extend_i32_u();
    i.i64_add().local_set(v);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(neg).if_(BlockType::Result(ValType::I64));
    i.i64_const(0).local_get(v).i64_sub();
    i.else_();
    i.local_get(v);
    i.end();
    i.end();
    f
}

/// `(ptr, len, out) -> ok`: Rust's `Ipv4Addr` grammar — exactly four parts
/// separated by `.`, each one to three ASCII digits, at most 255, no leading
/// zero unless the part is `0` — the four bytes at `out`.
fn net_fn_v4() -> Function {
    let (ptr, len, out) = (0u32, 1u32, 2u32);
    let (k, part, digits, val, first, c) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.block(BlockType::Empty).loop_(BlockType::Empty); // parts
    i.i32_const(0).local_set(digits);
    i.i32_const(0).local_set(val);
    i.i32_const(0).local_set(first);
    i.block(BlockType::Empty).loop_(BlockType::Empty); // digits
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('0' as i32).i32_sub().local_tee(c);
    i.i32_const(9).i32_gt_u().br_if(1);
    i.local_get(digits).i32_eqz().if_(BlockType::Empty);
    i.local_get(c).i32_const(1).i32_add().local_set(first); // 1 = a leading `0`
    i.end();
    // A fourth digit already fails; stop the value from wrapping.
    i.local_get(digits).i32_const(3).i32_lt_u().if_(BlockType::Empty);
    i.local_get(val).i32_const(10).i32_mul().local_get(c).i32_add().local_set(val);
    i.end();
    i.local_get(digits).i32_const(1).i32_add().local_set(digits);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    // digits in 1..=3, val <= 255, and a leading zero only alone.
    i.local_get(digits).i32_eqz();
    i.local_get(digits).i32_const(3).i32_gt_u().i32_or();
    i.local_get(val).i32_const(255).i32_gt_u().i32_or();
    i.local_get(first).i32_const(1).i32_eq().local_get(digits).i32_const(1).i32_gt_u().i32_and().i32_or();
    i.if_(BlockType::Empty).i32_const(0).return_().end();
    i.local_get(out).local_get(part).i32_add().local_get(val).i32_store8(net_m8(0));
    i.local_get(part).i32_const(1).i32_add().local_tee(part);
    i.i32_const(4).i32_eq().br_if(1);
    i.local_get(k).local_get(len).i32_lt_u().if_(BlockType::Empty);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('.' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(2); // the next part
    i.end();
    i.end();
    i.i32_const(0).return_();
    i.end().end();
    i.local_get(k).local_get(len).i32_eq();
    i.end();
    f
}

/// `(ptr, len, tmp) -> count | v4 << 8, or -1`: one side of an IPv6 `::`
/// (or the whole address without one) as its groups at `tmp` (u16 each; an
/// IPv4 tail is two, and only the last part may be one — its bytes staged
/// at `tmp + 16`). Empty text is no groups; more than eight is -1.
fn net_fn_side(v4: u32) -> Function {
    let (ptr, len, tmp) = (0u32, 1u32, 2u32);
    let (k, cnt, end, dot, val, c, plen) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(len).i32_eqz().if_(BlockType::Empty).i32_const(0).return_().end();
    i.loop_(BlockType::Empty); // parts
    // end = the next `:` (or len); dot = whether the part holds a `.`.
    i.local_get(k).local_set(end);
    i.i32_const(0).local_set(dot);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(end).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(end).i32_add().i32_load8_u(net_m8(0)).local_tee(c);
    i.i32_const(':' as i32).i32_eq().br_if(1);
    i.local_get(c).i32_const('.' as i32).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(dot);
    i.end();
    i.local_get(end).i32_const(1).i32_add().local_set(end);
    i.br(0).end().end();
    i.local_get(end).local_get(k).i32_sub().local_set(plen);
    i.local_get(dot).if_(BlockType::Empty);
    // An IPv4 tail: the last part, two free groups, a valid address.
    i.local_get(end).local_get(len).i32_ne();
    i.local_get(cnt).i32_const(6).i32_gt_u().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(ptr).local_get(k).i32_add().local_get(plen).local_get(tmp).i32_const(16).i32_add().call(v4);
    i.i32_eqz().if_(BlockType::Empty).i32_const(-1).return_().end();
    for (g, hi, lo) in [(0u32, 16u64, 17u64), (1, 18, 19)] {
        i.local_get(tmp).local_get(cnt).i32_const(g as i32).i32_add().i32_const(1).i32_shl().i32_add();
        i.local_get(tmp).i32_load8_u(net_m8(hi)).i32_const(8).i32_shl();
        i.local_get(tmp).i32_load8_u(net_m8(lo)).i32_or();
        i.i32_store16(net_m16(0));
    }
    i.local_get(cnt).i32_const(2).i32_add().i32_const(256).i32_or().return_();
    i.end();
    // A hex group: one to four digits, a ninth group is -1.
    i.local_get(plen).i32_eqz();
    i.local_get(plen).i32_const(4).i32_gt_u().i32_or();
    i.local_get(cnt).i32_const(8).i32_ge_u().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.i32_const(0).local_set(val);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(end).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).local_set(c);
    // c -> its hex value, or 16 for a non-hex byte.
    i.local_get(c).i32_const('0' as i32).i32_sub().i32_const(10).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(c).i32_const('0' as i32).i32_sub();
    i.else_();
    i.local_get(c).i32_const(0x20).i32_or().i32_const('a' as i32).i32_sub().i32_const(6).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(c).i32_const(0x20).i32_or().i32_const('a' as i32 - 10).i32_sub();
    i.else_();
    i.i32_const(16);
    i.end();
    i.end();
    i.local_tee(c).i32_const(15).i32_gt_u().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(val).i32_const(4).i32_shl().local_get(c).i32_or().local_set(val);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(tmp).local_get(cnt).i32_const(1).i32_shl().i32_add().local_get(val).i32_store16(net_m16(0));
    i.local_get(cnt).i32_const(1).i32_add().local_set(cnt);
    i.local_get(end).local_get(len).i32_eq().if_(BlockType::Empty).local_get(cnt).return_().end();
    // Past the `:`; an empty next part fails as a zero-length group.
    i.local_get(end).i32_const(1).i32_add().local_set(k);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// `(ptr, len, el) -> 0 v4 / 1 v6 / -1`: Rust's `IpAddr` grammar (no `:`
/// means IPv4); the address written at `el` as an `ip-address` element. An
/// IPv6 `::` stands for at least one zero group, at most one `::` appears,
/// and an IPv4 tail ends the address — never the head before a `::`
/// (Rust's `read_ipv6_addr` refuses it there).
fn net_fn_ip_parse(a: &NetAbi, t: &NetTexts, fns: NetFns) -> Function {
    let (ptr, len, el) = (0u32, 1u32, 2u32);
    let (k, d, rh, rt, h, tl, j) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let (head, tail) = (t.scratch(NET_SCR_HEAD), t.scratch(NET_SCR_TAIL));
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    // d = the first `::`, or -1; k scans for any `:`.
    i.i32_const(-1).local_set(d);
    i.i32_const(-1).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(0)).i32_const(':' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(k).i32_const(0).i32_lt_s().if_(BlockType::Empty).local_get(j).local_set(k).end();
    i.local_get(d).i32_const(0).i32_lt_s();
    i.local_get(j).i32_const(1).i32_add().local_get(len).i32_lt_u().i32_and().if_(BlockType::Empty);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(1)).i32_const(':' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(j).local_set(d);
    i.local_get(j).i32_const(1).i32_add().local_set(j); // `::` is one separator
    i.end();
    i.end();
    i.end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    // IPv4.
    i.local_get(k).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(ptr).local_get(len).local_get(el).i32_const(a.ia_payload as i32).i32_add().call(fns.v4);
    i.if_(BlockType::Empty);
    i.local_get(el).i32_const(a.ia_case[0]).i32_store8(net_m8(0));
    i.i32_const(0).return_();
    i.end();
    i.i32_const(-1).return_();
    i.end();
    // IPv6: a second `::` (past the first) fails.
    i.local_get(d).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(d).i32_const(2).i32_add().local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_const(1).i32_add().local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(0)).i32_const(':' as i32).i32_eq();
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(1)).i32_const(':' as i32).i32_eq().i32_and();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    // The eight groups start zeroed (the `::` run).
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(0).i32_const(16).memory_fill(0);
    i.local_get(d).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    // No `::`: exactly eight groups.
    i.local_get(ptr).local_get(len).i32_const(head).call(fns.side).local_tee(rh);
    i.i32_const(0).i32_lt_s();
    i.local_get(rh).i32_const(255).i32_and().i32_const(8).i32_ne().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(head).i32_const(16).memory_copy(0, 0);
    i.else_();
    // head :: tail — at most seven groups between them, no IPv4 head.
    i.local_get(ptr).local_get(d).i32_const(head).call(fns.side).local_tee(rh);
    i.i32_const(0).i32_lt_s();
    i.local_get(rh).i32_const(256).i32_and().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(ptr).local_get(d).i32_add().i32_const(2).i32_add();
    i.local_get(len).local_get(d).i32_sub().i32_const(2).i32_sub();
    i.i32_const(tail).call(fns.side).local_tee(rt);
    i.i32_const(0).i32_lt_s().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(rh).i32_const(255).i32_and().local_set(h);
    i.local_get(rt).i32_const(255).i32_and().local_set(tl);
    i.local_get(h).local_get(tl).i32_add().i32_const(7).i32_gt_u().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(head).local_get(h).i32_const(1).i32_shl().memory_copy(0, 0);
    i.local_get(el).i32_const(a.ia_payload as i32 + 16).i32_add().local_get(tl).i32_const(1).i32_shl().i32_sub();
    i.i32_const(tail).local_get(tl).i32_const(1).i32_shl().memory_copy(0, 0);
    i.end();
    i.local_get(el).i32_const(a.ia_case[1]).i32_store8(net_m8(0));
    i.i32_const(1);
    i.end();
    f
}

/// `(dst, el) -> dst_end`: an `ip-address` element as Rust's `Display`
/// writes it — dotted IPv4; IPv6 as `::ffff:a.b.c.d` when IPv4-mapped, else
/// lowercase groups without leading zeros, the longest run of two or more
/// zero groups (the first when tied) as `::`.
fn net_fn_ip_fmt(a: &NetAbi, t: &NetTexts, fns: NetFns) -> Function {
    let (dst, el) = (0u32, 1u32);
    let (k, g, cs, cl, bs, bl, sh, end) = (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let po = a.ia_payload;
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();
    let group = |i: &mut Sink<'_>, k: u32| {
        i.local_get(el).local_get(k).i32_const(1).i32_shl().i32_add().i32_load16_u(net_m16(po));
    };
    i.local_get(el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().if_(BlockType::Empty);
    net_dotted(&mut i, dst, fns.dec, &|i: &mut Sink<'_>, b: u64| {
        i.local_get(el).i32_load8_u(net_m8(po + b));
    });
    i.local_get(dst).return_();
    i.end();
    // IPv4-mapped: five zero groups, then 0xffff.
    i.i32_const(1);
    for gk in 0..5u64 {
        i.local_get(el).i32_load16_u(net_m16(po + 2 * gk)).i32_eqz().i32_and();
    }
    i.local_get(el).i32_load16_u(net_m16(po + 10)).i32_const(0xffff).i32_eq().i32_and();
    i.if_(BlockType::Empty);
    net_put(&mut i, dst, t.mapped);
    net_dotted(&mut i, dst, fns.dec, &|i: &mut Sink<'_>, b: u64| {
        i.local_get(el).i32_load16_u(net_m16(po + 12 + 2 * (b / 2)));
        if b % 2 == 0 {
            i.i32_const(8).i32_shr_u();
        } else {
            i.i32_const(255).i32_and();
        }
    });
    i.local_get(dst).return_();
    i.end();
    // The longest zero run: (bs, bl); the current one (cs, cl).
    i.i32_const(-1).local_set(bs);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).i32_const(8).i32_ge_u().br_if(1);
    group(&mut i, k);
    i.i32_eqz().if_(BlockType::Empty);
    i.local_get(cl).i32_eqz().if_(BlockType::Empty).local_get(k).local_set(cs).end();
    i.local_get(cl).i32_const(1).i32_add().local_tee(cl);
    i.local_get(bl).i32_gt_u().if_(BlockType::Empty);
    i.local_get(cs).local_set(bs);
    i.local_get(cl).local_set(bl);
    i.end();
    i.else_();
    i.i32_const(0).local_set(cl);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(bl).i32_const(2).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(-1).local_set(bs);
    i.end();
    // Groups 0..8, `:` between, the run (when bs >= 0) written as `::`.
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).i32_const(8).i32_ge_u().br_if(1);
    i.local_get(k).local_get(bs).i32_eq().if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b':');
    net_put_byte(&mut i, dst, b':');
    i.local_get(k).local_get(bl).i32_add().local_set(k);
    i.br(1); // the loop's next group
    i.end();
    // A `:` before every group but the first and the one right after a run.
    i.local_get(k).i32_const(0).i32_ne();
    i.local_get(bs).i32_const(0).i32_lt_s();
    i.local_get(k).local_get(bs).local_get(bl).i32_add().i32_ne().i32_or().i32_and();
    i.if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b':');
    i.end();
    // The group's hex without leading zeros: from the highest nonzero nibble.
    group(&mut i, k);
    i.local_set(g);
    i.i32_const(12).local_set(sh);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(sh).i32_eqz().br_if(1);
    i.local_get(g).local_get(sh).i32_shr_u().br_if(1);
    i.local_get(sh).i32_const(4).i32_sub().local_set(sh);
    i.br(0).end().end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(g).local_get(sh).i32_shr_u().local_set(end);
    i.local_get(dst);
    i.local_get(end).i32_const(15).i32_and();
    i.local_get(end).i32_const(15).i32_and().i32_const(10).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32)).i32_const('0' as i32).else_().i32_const('a' as i32 - 10).end();
    i.i32_add().i32_store8(net_m8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.local_get(sh).i32_eqz().br_if(1);
    i.local_get(sh).i32_const(4).i32_sub().local_set(sh);
    i.br(0).end().end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(dst);
    i.end();
    f
}

/// Four bytes as `a.b.c.d`: `byte_at` pushes byte `b` (an i32).
fn net_dotted(i: &mut Sink<'_>, dst: u32, dec: u32, byte_at: &dyn Fn(&mut Sink<'_>, u64)) {
    for b in 0..4u64 {
        if b > 0 {
            net_put_byte(i, dst, b'.');
        }
        i.local_get(dst);
        byte_at(i, b);
        i.i64_extend_i32_u().call(dec).local_set(dst);
    }
}

/// `(el, port) -> ()`: the flat `ip-socket-address` operands of `el` at
/// `port` (flow-info and scope-id 0), one i32 per slot at NET_SCR_SA.
fn net_fn_sa(a: &NetAbi, t: &NetTexts) -> Function {
    let (el, port) = (0u32, 1u32);
    let at = t.scratch(NET_SCR_SA);
    let slot = |s: usize| (at + 4 * s as i32) as u64;
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.i32_const(at).i32_const(0).i32_const(4 * a.flat_len as i32).memory_fill(0);
    i.local_get(el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i32_const(a.sa_case[0]).i32_store(net_m32(slot(0)));
    i.i32_const(0).local_get(port).i32_store(net_m32(slot(a.flat4[0])));
    for b in 0..4 {
        i.i32_const(0).local_get(el).i32_load8_u(net_m8(a.ia_payload + b as u64)).i32_store(net_m32(slot(a.flat4[1 + b])));
    }
    i.else_();
    i.i32_const(0).i32_const(a.sa_case[1]).i32_store(net_m32(slot(0)));
    i.i32_const(0).local_get(port).i32_store(net_m32(slot(a.flat6[0])));
    for g in 0..8 {
        i.i32_const(0).local_get(el).i32_load16_u(net_m16(a.ia_payload + 2 * g as u64)).i32_store(net_m32(slot(a.flat6[2 + g])));
    }
    i.end();
    i.end();
    f
}

/// `(host, len) -> count`: the addresses `host` names, as native's
/// `almide_net_resolve` finds them — `[...]` stripped, an IP literal itself
/// (parsed here, never sent to the resolver), else `resolve-addresses`; the
/// `ip-address` list at `g.addrs`. 0 = nothing (the resolver's err included:
/// native reads every resolver failure as unresolvable).
fn net_fn_resolve(g: P3Globals, a: &NetAbi, t: &NetTexts, parts: NetParts) -> Function {
    let NetParts { fns, ng, imp } = parts;
    let (ptr, len) = (0u32, 1u32);
    let ret = (g.park + STATRET) as i32;
    let el = t.scratch(NET_SCR_EL);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(len).i32_const(2).i32_ge_u().if_(BlockType::Empty);
    i.local_get(ptr).i32_load8_u(net_m8(0)).i32_const('[' as i32).i32_eq();
    i.local_get(ptr).local_get(len).i32_add().i32_const(1).i32_sub().i32_load8_u(net_m8(0)).i32_const(']' as i32).i32_eq();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(ptr).i32_const(1).i32_add().local_set(ptr);
    i.local_get(len).i32_const(2).i32_sub().local_set(len);
    i.end();
    i.end();
    i.local_get(ptr).local_get(len).i32_const(el).call(fns.ip_parse).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.i32_const(el).global_set(ng.addrs);
    i.i32_const(1).return_();
    i.end();
    i.i32_const(NET_LOOKUP_RESERVE).call(g.f_reserve);
    i.local_get(ptr).local_get(len).i32_const(ret).call(imp.at(NI_RESOLVE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty).i32_const(0).return_().end();
    i.i32_const(ret).i32_load(net_m32(a.lookup_payload)).global_set(ng.addrs);
    i.i32_const(ret).i32_load(net_m32(a.lookup_payload + 4));
    i.end();
    f
}
