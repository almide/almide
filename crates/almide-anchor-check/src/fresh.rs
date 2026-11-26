//! FRESH-BLOCK accesses: the cutter for `proofs/GuardFresh.v`.
//!
//! The most common store of a module writes into a block the function has
//! just allocated: a record, a variant, a closure or a list literal being
//! filled. In the shipped bytes that is `i32.const n ; call $alloc ;
//! local.set x`, then any instructions that leave `x` alone, then an access
//! whose address is `x` plus a constant. The proof script takes those three
//! byte strings, reads the first as the allocation of `n` payload bytes into
//! `x`, scans the second with the length of every instruction (none assigns
//! `x`, and the nesting ends where it began and never drops below it), runs
//! the third symbolically, and accepts when the access lies inside the
//! header and the `n` payload bytes; its theorem (`fresh_sound`) then puts
//! the access inside `[x, x + 12 + n)` in every run of the third string,
//! with the machine's 32-bit wrap-around. No witness is read.
//!
//! This file only CUTS, and it tests NO bound: every access of the shape is
//! proposed, so a store past its block is a proposal the kernel REFUSES
//! (`proofs/check-shipped-fresh.sh` fails on one) and not an access that
//! quietly goes uncounted.
//!
//! TRUSTED of this file: that the three byte strings it prints are
//! consecutive bytes of ONE function body, cut at instruction boundaries
//! (the offsets are wasmparser's), and that the allocator's index is the one
//! the caller names. That the function at that index IS the proven
//! allocator is not this file's word: `proofs/check-shipped-routines.sh` has
//! the kernel decode it to the proven tree.

use wasmparser::Operator as Op;

use crate::decode::Body;

/// One proposal: the function's position among the defined functions, the
/// access instruction's ordinal in it, and the three byte strings.
pub struct Fresh {
    pub func: usize,
    pub access: usize,
    pub pre: Vec<u8>,
    pub mid: Vec<u8>,
    pub tail: Vec<u8>,
}

/// [`fresh`] of the module `wasm`, decoded and validated here.
pub fn shipped(wasm: &[u8], alloc: u32) -> Result<Vec<Fresh>, String> {
    let bodies = crate::decode(wasm)?;
    Ok(fresh(wasm, &bodies, alloc))
}

/// The local an address expression ending at `p` reads, and where the
/// expression starts: `local.get x`, or that plus a constant in either order.
fn base(b: &Body<'_>, p: usize) -> Option<(u32, usize)> {
    match &b.ops[p] {
        Op::LocalGet { local_index } => Some((*local_index, p)),
        Op::I32Add if p >= 2 => match (&b.ops[p - 2], &b.ops[p - 1]) {
            (Op::LocalGet { local_index }, Op::I32Const { .. }) | (Op::I32Const { .. }, Op::LocalGet { local_index }) => Some((*local_index, p - 2)),
            _ => None,
        },
        _ => None,
    }
}

/// Every access of `bodies` whose address is a local plus a constant, where
/// the latest assignment of that local before it is `i32.const n ; call
/// alloc ; local.set` (or `local.tee`) at the access's own control depth and
/// nothing in between leaves that depth's block. `wasm` is the module the
/// bodies were decoded from.
pub fn fresh(wasm: &[u8], bodies: &[Body<'_>], alloc: u32) -> Vec<Fresh> {
    let mut out = Vec::new();
    for (func, b) in bodies.iter().enumerate() {
        let end_of = |k: usize| b.at.get(k + 1).copied().unwrap_or(b.end);
        for (k, op) in b.ops.iter().enumerate() {
            if !crate::window::is_access(op) {
                continue;
            }
            let Some((x, s0)) = b.args[k].first().copied().flatten().and_then(|p| base(b, p)) else { continue };
            let assigns = |t: &usize| matches!(&b.ops[*t], Op::LocalSet { local_index } | Op::LocalTee { local_index } if *local_index == x);
            let Some(j) = (0..s0).rev().find(assigns) else { continue };
            if j < 2 || b.depth[j] != b.depth[k] || b.depth[j - 2] != b.depth[j] || (j..=k).any(|t| b.depth[t] < b.depth[j]) {
                continue;
            }
            let allocated = matches!(&b.ops[j - 1], Op::Call { function_index } if *function_index == alloc) && matches!(&b.ops[j - 2], Op::I32Const { .. });
            if !allocated || !crate::window::reads(b, s0, k) {
                continue;
            }
            out.push(Fresh {
                func,
                access: k,
                pre: wasm[b.at[j - 2]..end_of(j)].to_vec(),
                mid: wasm[end_of(j)..b.at[s0]].to_vec(),
                tail: wasm[b.at[s0]..end_of(k)].to_vec(),
            });
        }
    }
    out
}
