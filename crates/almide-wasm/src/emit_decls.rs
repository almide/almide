//! The emission pass's declaration table (#2759, witness_decls.rs), split
//! from emit.rs: each program fn's, `main`'s and lifted lambda's index with
//! what its source declares, and which body creates which closure.

use std::ops::Range;

use almide_ir::IrFunction;

use crate::*;

/// Which body lifted which lambdas, as emission recorded it: a lambda is
/// lifted while the body that builds it lowers, so the range a body leaves
/// is the closures it creates (`work.lifted` positions, the `j` of
/// `TableEntry::Lambda(j)`).
pub(super) struct Creators<'a> {
    /// Per program fn (index `i`, at `F_FN_BASE + i`).
    pub(super) fns: &'a [Range<usize>],
    /// `main`'s.
    pub(super) main: Range<usize>,
    /// Per lifted lambda: the lambdas its own body lifted.
    pub(super) lambdas: &'a [Range<usize>],
}

impl Creators<'_> {
    /// `(creator, closure)` pairs in the pre-declare index space, each
    /// closure a lambda that is a funcref-table entry. A creator lambda that
    /// is no table entry has no index here, and its closures are left out:
    /// unattributed, the capability witness charges their callers instead.
    fn creations(&self, work: &FnWork, entry_fn_indices: &[u32], main_index: u32) -> Vec<(u32, u32)> {
        let lambda_at: std::collections::HashMap<usize, u32> = work
            .entries
            .borrow()
            .iter()
            .zip(entry_fn_indices)
            .filter_map(|(e, &idx)| match e {
                TableEntry::Lambda(j) => Some((*j as usize, idx)),
                _ => None,
            })
            .collect();
        let owners = self
            .fns
            .iter()
            .enumerate()
            .map(|(i, r)| (Some(F_FN_BASE + i as u32), r.clone()))
            .chain(std::iter::once((Some(main_index), self.main.clone())))
            .chain(self.lambdas.iter().enumerate().map(|(k, r)| (lambda_at.get(&k).copied(), r.clone())));
        let mut out = Vec::new();
        for (owner, made) in owners {
            let Some(owner) = owner else { continue };
            out.extend(made.filter_map(|j| lambda_at.get(&j)).map(|&idx| (owner, idx)));
        }
        out
    }
}

/// #2759: the declaration table the name and capability witnesses read
/// (witness_decls.rs) — each program fn's, `main`'s and lifted lambda's
/// index with what its source declares.
pub(super) fn record_decls(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    (main, main_index): (Option<&IrFunction>, u32),
    work: &FnWork,
    entry_fn_indices: &[u32],
    stubs: &[imports::Declared],
    creators: &Creators<'_>,
    bytes: &[u8],
) {
    use crate::witness::decls::{DeclFn, Declared, PassDecls};
    if !crate::witness::decls::collecting() {
        return;
    }
    let stub_declared = stubs.iter().map(|d| if d.effect { Declared::Effect } else { Declared::Pure }).collect();
    let stubs = stubs.iter().map(|d| d.index).collect();
    // #3041: a fn synthesized from an effect fn's body keeps its origin's
    // declaration (`IrFunction::declares_effect`).
    let class = |f: &IrFunction| Some(if f.declares_effect() { Declared::Effect } else { Declared::Pure });
    let meter = work.meter_reads.borrow();
    let reads = |name: &str| meter.get(name).copied().unwrap_or(0);
    let mut fns: Vec<DeclFn> = program_fns
        .iter()
        .enumerate()
        .map(|(i, (f, qual, _))| DeclFn {
            index: F_FN_BASE + i as u32,
            name: qual.clone().unwrap_or_else(|| f.name.as_str().to_string()),
            // A self-host registry body implements a SURFACE whose
            // declaration its callers were checked against; its own `fn`
            // keyword declares nothing about the host (random.int is an
            // `effect fn` implemented by a plain `fn random_int`).
            declared: if qual.as_deref().is_some_and(|q| q.starts_with("__selfhost_")) { None } else { class(f) },
            meter_clock_reads: reads(&qual.clone().unwrap_or_else(|| f.name.as_str().to_string())),
        })
        .collect();
    fns.push(DeclFn {
        index: main_index,
        name: "main".into(),
        declared: main.map_or(Some(Declared::Pure), class),
        meter_clock_reads: reads("main"),
    });
    for (pos, e) in work.entries.borrow().iter().enumerate() {
        if let (TableEntry::Lambda(j), Some(&index)) = (e, entry_fn_indices.get(pos)) {
            let name = format!("<lambda#{j}>");
            let meter_clock_reads = reads(&name);
            fns.push(DeclFn { index, name, declared: None, meter_clock_reads });
        }
    }
    let creations = creators.creations(work, entry_fn_indices, main_index);
    crate::witness::decls::record(PassDecls { fns, stubs, stub_declared, creations, bytes: bytes.to_vec() });
}

