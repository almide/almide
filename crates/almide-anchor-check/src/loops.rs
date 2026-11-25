//! LOOP reads: the cutter for `proofs/GuardLoop.v`.
//!
//! A `for` over a list or a string does not ship as a window (window.rs): the
//! count is read ONCE, before the loop, into a local, and every turn tests
//! the counter against that local and leaves the loop when it is not below.
//! In the shipped bytes that is the capture `local.get b ; i32.load 4 ;
//! [i32.const s ; i32.div_u ;] local.set n`, then anything, then in one
//! straight-line run the exit test `local.get i ; local.get n ; i32.ge_u ;
//! br_if l` and an access whose address is built from `b` and `i`. The proof
//! script takes four byte strings: the capture, everything up to the test,
//! the test and the access, and everything from there to the `end` that
//! closes the outermost block opened since the capture. It runs GuardWindow's
//! check on the capture's instructions followed by those of the test and the
//! access, scans the rest with the length of every instruction (none assigns
//! `b` or `n`, and the nesting closes what it opened and never leaves the
//! block the capture stands in), and reads which of three things stands
//! behind the block's length word still being the one the capture read: no
//! instruction in between writes memory or calls, the capture is preceded by
//! `local.get b ; call $inc`, or `b` is read nowhere but for the access. Its
//! theorem (`loop_sound`) then puts the access inside the block's payload in
//! every run of the test and the access from locals in which `n` holds the
//! captured count, with the machine's 32-bit wrap-around. No witness is read.
//!
//! This file only CUTS, and it tests NO bound, stride, offset or width:
//! every access addressed by the captured block's local and the tested
//! counter is proposed, so one the kernel cannot place inside the block is a
//! proposal it REFUSES (`proofs/check-shipped-loops.sh` fails on one) and not
//! an access that quietly goes uncounted.
//!
//! TRUSTED of this file: that the byte strings it prints are consecutive
//! bytes of ONE function body, cut at instruction boundaries (the offsets are
//! wasmparser's), and that the increment routine's index is the one the
//! caller names. That the function at that index IS the proven increment is
//! not this file's word: `proofs/check-shipped-routines.sh` has the kernel
//! decode it to the proven tree.

use wasmparser::Operator as Op;

use crate::decode::Body;

/// One proposal: the function's position among the defined functions, the
/// access instruction's ordinal in it, and the byte strings. `hold` is
/// `local.get b ; call $inc` when that stands right before the capture, and
/// empty otherwise.
pub struct LoopRead {
    pub func: usize,
    pub access: usize,
    pub hold: Vec<u8>,
    pub pre: Vec<u8>,
    pub mid: Vec<u8>,
    pub ht: Vec<u8>,
    pub rest: Vec<u8>,
}

/// [`loops`] of the module `wasm`, decoded and validated here.
pub fn shipped(wasm: &[u8], inc: Option<u32>) -> Result<Vec<LoopRead>, String> {
    let bodies = crate::decode(wasm)?;
    Ok(loops(wasm, &bodies, inc))
}

/// The locals an address expression ending at `p` is built from, by adds,
/// multiplies and shifts over constants; `None` when anything else is in it.
fn leaves(b: &Body<'_>, p: usize) -> Option<Vec<u32>> {
    let (mut out, mut todo) = (Vec::new(), vec![p]);
    while let Some(u) = todo.pop() {
        match &b.ops[u] {
            Op::LocalGet { local_index } => out.push(*local_index),
            Op::I32Const { .. } => {}
            Op::I32Add | Op::I32Mul | Op::I32Shl => {
                for a in &b.args[u] {
                    todo.push((*a)?);
                }
            }
            _ => return None,
        }
    }
    out.sort_unstable();
    Some(out)
}

/// Every load and store that follows, in one straight-line run at its own
/// depth, an exit test `local.get i ; local.get n ; i32.ge_u ; br_if`, where
/// `n` was last assigned by a capture of a block's count, nothing between
/// the capture and the access leaves the block the capture stands in, and
/// the access's address is built from constants, the captured block's local
/// and the tested counter. `inc` is the increment routine's function index,
/// as a `call` names it.
pub fn loops(wasm: &[u8], bodies: &[Body<'_>], inc: Option<u32>) -> Vec<LoopRead> {
    let mut out = Vec::new();
    for (func, b) in bodies.iter().enumerate() {
        let end_of = |k: usize| b.at.get(k + 1).copied().unwrap_or(b.end);
        let is_len = |u: usize| matches!(&b.ops[u], Op::I32Load { memarg } if memarg.offset == 4);
        for (k, op) in b.ops.iter().enumerate() {
            if !crate::window::is_access(op) {
                continue;
            }
            let d = b.depth[k];
            // The exit test: the nearest conditional branch before the access
            // at its depth, fed by `local.get i ; local.get n ; i32.ge_u`.
            let Some(t) = (0..k).rev().find(|&t| b.depth[t] == d && matches!(b.ops[t], Op::BrIf { .. })) else { continue };
            if t < 3 || !matches!(b.ops[t - 1], Op::I32GeU) {
                continue;
            }
            let (Op::LocalGet { local_index: counter }, Op::LocalGet { local_index: n }) = (&b.ops[t - 3], &b.ops[t - 2]) else { continue };
            let (counter, n, s0) = (*counter, *n, t - 3);
            if !crate::window::reads_past_exits(b, s0, k) {
                continue;
            }
            // The capture: the latest assignment of the count's local before
            // the test.
            let Some(j) = (0..s0).rev().find(|&u| matches!(&b.ops[u], Op::LocalSet { local_index } | Op::LocalTee { local_index } if *local_index == n)) else { continue };
            if !matches!(b.ops[j], Op::LocalSet { .. }) {
                continue;
            }
            let p0 = if j >= 2 && is_len(j - 1) {
                j - 2
            } else if j >= 4 && matches!(b.ops[j - 1], Op::I32DivU | Op::I32ShrU) && matches!(b.ops[j - 2], Op::I32Const { .. }) && is_len(j - 3) {
                j - 4
            } else {
                continue;
            };
            let Op::LocalGet { local_index: base } = &b.ops[p0] else { continue };
            // The access addresses the captured block by the tested counter.
            let Some(p) = b.args[k].first().copied().flatten() else { continue };
            let mut want = vec![*base, counter];
            want.sort_unstable();
            if leaves(b, p) != Some(want) {
                continue;
            }
            let d0 = b.depth[p0];
            if (p0..=j).any(|u| b.depth[u] != d0) {
                continue;
            }
            // From the capture to the access nothing leaves the block the
            // capture stands in (an `end` or `else` there has that depth).
            if (j + 1..=k).any(|u| b.depth[u] < d0 || (b.depth[u] == d0 && matches!(b.ops[u], Op::End | Op::Else))) {
                continue;
            }
            // The `end` of the outermost block opened since the capture.
            let e = if d == d0 {
                k
            } else {
                let Some(e) = (k + 1..b.ops.len()).find(|&u| b.depth[u] == d0 + 1 && matches!(b.ops[u], Op::End)) else { continue };
                e
            };
            let held = p0 >= 2
                && b.depth[p0 - 2] == d0
                && matches!(&b.ops[p0 - 2], Op::LocalGet { local_index } if local_index == base)
                && matches!(&b.ops[p0 - 1], Op::Call { function_index } if Some(*function_index) == inc);
            out.push(LoopRead {
                func,
                access: k,
                hold: if held { wasm[b.at[p0 - 2]..b.at[p0]].to_vec() } else { Vec::new() },
                pre: wasm[b.at[p0]..end_of(j)].to_vec(),
                mid: wasm[end_of(j)..b.at[s0]].to_vec(),
                ht: wasm[b.at[s0]..end_of(k)].to_vec(),
                rest: wasm[end_of(k)..end_of(e)].to_vec(),
            });
        }
    }
    out
}
