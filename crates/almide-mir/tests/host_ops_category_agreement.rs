//! #4916: the native certificate's host-op capabilities and the checker's
//! categories are two declarations of what one stdlib call reaches. The
//! checker admits a call under `[permissions].allow` by its categories
//! (`almide_ir::effect::stdlib_call_effect`: the fn's `@category`, else its
//! module's row); the certificate counts `HostOp::caps` at the call site. When
//! they differ, a program the checker admits can carry a capability witness
//! that names a different authority (fs.temp_dir and unix.shm_create did, until
//! both tables were edited by hand). This gate reads every row of `HOST_OPS` —
//! the rows themselves, no list kept here — and holds the two declarations to
//! one map from category leaf to capability.

use almide_ir::effect::{stdlib_call_effect, Effect};
use almide_mir::host_ops::HOST_OPS;
use almide_mir::Capability;
use std::collections::BTreeSet;

/// The capability a category leaf is counted as, `None` for a leaf the
/// certificate has no capability for (a fan, an abort: neither is a host op
/// row).
fn leaf_capability(e: Effect) -> Option<Capability> {
    Some(match e {
        Effect::FsRead => Capability::FsRead,
        Effect::FsWrite => Capability::FsWrite,
        Effect::NetFetch | Effect::NetListen => Capability::Net,
        Effect::IoStdin => Capability::Stdin,
        // stderr rides the output capability (host_ops.rs: `io.eprint`).
        Effect::IoStdout | Effect::IoStderr => Capability::Stdout,
        Effect::EnvRead | Effect::EnvWrite => Capability::CliArgs,
        Effect::Proc => Capability::Process,
        Effect::Time => Capability::Clock,
        Effect::Rand => Capability::Entropy,
        Effect::Fan | Effect::Abort => return None,
    })
}

/// The capabilities the checker's categories for `module.func` stand for.
fn checker_caps(module: &str, func: &str) -> BTreeSet<Capability> {
    stdlib_call_effect(module, func).into_iter().filter_map(leaf_capability).collect()
}

/// Every disagreement between the two tables, one line each.
fn disagreements(rows: &[(&str, &str, BTreeSet<Capability>)]) -> Vec<String> {
    rows.iter()
        .filter_map(|(m, f, caps)| {
            let cats = checker_caps(m, f);
            (cats != *caps).then(|| format!("{m}.{f}: certificate {caps:?}, checker categories {cats:?}"))
        })
        .collect()
}

fn host_op_rows() -> Vec<(&'static str, &'static str, BTreeSet<Capability>)> {
    HOST_OPS.iter().map(|r| (r.module, r.func, r.caps.iter().copied().collect())).collect()
}

#[test]
fn every_host_op_counts_the_capabilities_its_categories_name() {
    let rows = host_op_rows();
    assert!(rows.len() > 50, "HOST_OPS read {} rows: the table moved", rows.len());
    let bad = disagreements(&rows);
    assert!(bad.is_empty(), "host op capabilities disagree with the checker's categories (#4916):\n{}", bad.join("\n"));
}

/// A capability an op touches without charging it is declared on its row
/// with a written reason (#4916), and is not also among the row's charged
/// capabilities.
#[test]
fn every_subsumed_capability_carries_a_reason_and_is_not_charged() {
    let subsumed: Vec<_> = HOST_OPS.iter().flat_map(|r| r.subsumed.iter().map(move |s| (r, s))).collect();
    assert!(!subsumed.is_empty(), "no row declares a subsumed capability: the field went unread");
    for (r, (cap, reason)) in subsumed {
        assert!(reason.split_whitespace().count() >= 5, "{}.{}: subsumed {cap:?} has no written reason", r.module, r.func);
        assert!(!r.caps.contains(cap), "{}.{}: {cap:?} is both charged and subsumed", r.module, r.func);
    }
}

/// The control: one row's capability set moved by one leaf — for every row,
/// both directions (a capability dropped, an unrelated one added) — is caught.
#[test]
fn a_row_whose_capabilities_drift_from_its_categories_is_caught() {
    let rows = host_op_rows();
    let all = [
        Capability::Stdout, Capability::Entropy, Capability::CliArgs, Capability::FsRead,
        Capability::FsWrite, Capability::Clock, Capability::Stdin, Capability::Net, Capability::Process,
    ];
    for (i, (m, f, caps)) in rows.iter().enumerate() {
        let extra = all.iter().copied().find(|c| !caps.contains(c)).expect("a capability the row lacks");
        let mut added = rows.clone();
        added[i].2.insert(extra);
        assert!(disagreements(&added).iter().any(|l| l.starts_with(&format!("{m}.{f}:"))), "{m}.{f} + {extra:?} not caught");
        if let Some(c) = caps.iter().next() {
            let mut dropped = rows.clone();
            dropped[i].2.remove(c);
            assert!(disagreements(&dropped).iter().any(|l| l.starts_with(&format!("{m}.{f}:"))), "{m}.{f} - {c:?} not caught");
        }
    }
}
