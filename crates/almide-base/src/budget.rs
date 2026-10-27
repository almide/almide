//! Compile-cost budgets (ADR-0030, #4136): the one table of every counted
//! limit the compiler holds its input to.
//!
//! The compiler runs on untrusted source — CI on a pull request, the LSP the
//! moment a file opens, graders, the playground — so each phase whose work
//! grows with its input has a named budget here, and exceeding it is a
//! defined diagnostic, never a crash, an out-of-memory kill or a hang.
//!
//! - **Counted, not timed (D2).** A budget counts nodes, instances, bytes or
//!   levels, never seconds or free memory: the same input gets the same
//!   verdict on every machine, natively and wasm-hosted, in the CLI and LSP.
//! - **The source cannot raise one (D3).** No attribute, manifest key, lock
//!   entry or environment variable reads into this table. The person running
//!   the compiler can, with `--budget NAME=N` on the command line (the only
//!   caller of [`set_override`]); `--profile critical` refuses that flag.
//! - **Set from measurement (D6).** Each default is about ten times the
//!   largest figure observed over `spec/`, `stdlib/` and the exercises corpus
//!   (`scripts/measure-compile-budgets.sh`, `proofs/compile-budgets.toml`).
//!
//! The figures each phase observed are kept as maxima ([`observe`]) and the
//! counted work as a running sum ([`add_work`]); `--budget-report` prints both,
//! which is what the measurement script and the blowup-shape gate
//! (`tests/compile_budget_test.rs`) read.

use std::sync::atomic::{AtomicU64, Ordering};

/// One budget row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The name `--budget NAME=N` takes.
    pub name: &'static str,
    /// The limit when nothing overrides it.
    pub default: u64,
    /// What is counted.
    pub unit: &'static str,
    /// The diagnostic that ends a run over the budget.
    pub diagnostic: &'static str,
    /// Where the count is taken and what it bounds.
    pub doc: &'static str,
}

/// A budget's position in [`BUDGETS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum BudgetId {
    Nesting = 0,
    TypeSize = 1,
    GenericInstances = 2,
    ManifestBytes = 3,
    DependencyCount = 4,
    DependencyDepth = 5,
    ExhaustivenessSteps = 6,
}

/// Every budget, in [`BudgetId`] order.
pub const BUDGETS: &[Budget] = &[
    Budget {
        name: "nesting",
        default: 500,
        unit: "levels",
        diagnostic: "parse error `expression nesting too deep (max N)`",
        doc: "syntactic nesting of one expression or pattern, counted by the parser as it descends",
    },
    Budget {
        name: "type_size",
        default: 256,
        unit: "nodes",
        diagnostic: "E111",
        doc: "nodes of one inferred type, counted as the checker builds an expression's type and as it resolves an inference variable — `let t1 = (t0, t0)` doubled 22 times was 3.7 GB (#4114)",
    },
    Budget {
        name: "generic_instances",
        default: 1000,
        unit: "instances",
        diagnostic: "E112",
        doc: "specialized instances of protocol-bounded generic fns one program monomorphizes, each instance's type arguments also held to `type_size`; polymorphic recursion would add a larger one per round without end",
    },
    Budget {
        name: "manifest_bytes",
        default: 1_048_576,
        unit: "bytes",
        diagnostic: "error `<file> is larger than N bytes`",
        doc: "size of one `almide.toml` or `almide.lock` read, the project's own or a dependency's, checked before and while it is read (#4194)",
    },
    Budget {
        name: "dependency_count",
        default: 1000,
        unit: "packages",
        diagnostic: "error `too many dependencies (N, max M)`",
        doc: "distinct packages one dependency resolution visits, the transitive closure included, counted before each is fetched",
    },
    Budget {
        name: "dependency_depth",
        default: 64,
        unit: "levels",
        diagnostic: "error `dependency chain too deep (N, max M)`",
        doc: "length of the chain of dependencies from the root package to the one being resolved, counted before it is fetched",
    },
    Budget {
        name: "exhaustiveness_steps",
        default: 400,
        unit: "steps",
        diagnostic: "E113",
        doc: "recursive steps of the exhaustiveness and reachability check of one `match` (pattern-matrix usefulness is exponential in the worst case)",
    },
];

const N: usize = 7;

/// `0` = no override.
static OVERRIDES: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
/// The largest figure each budget has observed in this process.
static OBSERVED: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
/// Counted work per budget: the nodes / instances / bytes the phase built.
static WORK: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];

/// A phase with no diagnostics channel of its own (monomorphization) records
/// here that it stopped at its budget, with the figure it reached; the route
/// that ran it reports the diagnostic ([`take_exceeded`]). `0` = not hit.
static EXCEEDED: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];

/// Record that a phase stopped at `id`'s budget, having counted `figure`.
pub fn exceed(id: BudgetId, figure: u64) {
    EXCEEDED[id as usize].fetch_max(figure.max(1), Ordering::Relaxed);
}

/// The figure a phase stopped at since the last call, if it did.
pub fn take_exceeded(id: BudgetId) -> Option<u64> {
    match EXCEEDED[id as usize].swap(0, Ordering::Relaxed) {
        0 => None,
        n => Some(n),
    }
}

impl BudgetId {
    pub fn row(self) -> &'static Budget {
        &BUDGETS[self as usize]
    }
}

/// The limit in force for `id`.
#[inline]
pub fn limit(id: BudgetId) -> u64 {
    match OVERRIDES[id as usize].load(Ordering::Relaxed) {
        0 => BUDGETS[id as usize].default,
        n => n,
    }
}

/// The row named `name`.
pub fn by_name(name: &str) -> Option<BudgetId> {
    const IDS: [BudgetId; N] = [
        BudgetId::Nesting,
        BudgetId::TypeSize,
        BudgetId::GenericInstances,
        BudgetId::ManifestBytes,
        BudgetId::DependencyCount,
        BudgetId::DependencyDepth,
        BudgetId::ExhaustivenessSteps,
    ];
    IDS.into_iter().find(|id| id.row().name == name)
}

/// Apply one `--budget NAME=N`. The command line is the only caller.
pub fn set_override(arg: &str) -> Result<(), String> {
    let (name, value) = arg
        .split_once('=')
        .ok_or_else(|| format!("`--budget {arg}`: expected NAME=N"))?;
    let id = by_name(name.trim()).ok_or_else(|| {
        let names: Vec<&str> = BUDGETS.iter().map(|b| b.name).collect();
        format!("`--budget {arg}`: no budget named `{name}` (the budgets: {})", names.join(", "))
    })?;
    let n: u64 = value
        .trim()
        .parse()
        .map_err(|_| format!("`--budget {arg}`: `{value}` is not a whole number"))?;
    if n == 0 {
        return Err(format!("`--budget {arg}`: a budget is at least 1"));
    }
    OVERRIDES[id as usize].store(n, Ordering::Relaxed);
    Ok(())
}

/// Record a figure the phase measured (the maximum is kept).
#[inline]
pub fn observe(id: BudgetId, value: u64) {
    OBSERVED[id as usize].fetch_max(value, Ordering::Relaxed);
}

/// Count work the phase did.
#[inline]
pub fn add_work(id: BudgetId, n: u64) {
    WORK[id as usize].fetch_add(n, Ordering::Relaxed);
}

/// `budget <name> observed=<max> work=<sum> limit=<n>` per row: what
/// `--budget-report` prints.
pub fn report() -> String {
    let mut out = String::new();
    for (i, b) in BUDGETS.iter().enumerate() {
        out.push_str(&format!(
            "budget {} observed={} work={} limit={}\n",
            b.name,
            OBSERVED[i].load(Ordering::Relaxed),
            WORK[i].load(Ordering::Relaxed),
            match OVERRIDES[i].load(Ordering::Relaxed) {
                0 => b.default,
                n => n,
            },
        ));
    }
    out
}

/// The markdown table `docs/specs/cli.md` embeds (`almide budgets --md`).
pub fn markdown_table() -> String {
    let mut out = String::from("| Budget | Default | Counts | Exceeding it |\n|---|---|---|---|\n");
    for b in BUDGETS {
        out.push_str(&format!("| `{}` | {} {} | {} | {} |\n", b.name, b.default, b.unit, b.doc, b.diagnostic));
    }
    out
}

/// One line per budget (`almide budgets`).
pub fn plain_table() -> String {
    BUDGETS
        .iter()
        .map(|b| format!("{}={} {}\n", b.name, b.default, b.unit))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_the_table() {
        for (i, b) in BUDGETS.iter().enumerate() {
            let id = by_name(b.name).expect("every row has an id");
            assert_eq!(id as usize, i, "{} is out of order", b.name);
        }
        assert_eq!(BUDGETS.len(), N);
    }

    #[test]
    fn an_override_must_name_a_row_and_a_positive_count() {
        assert!(set_override("no_such=3").is_err());
        assert!(set_override("type_size=0").is_err());
        assert!(set_override("type_size=x").is_err());
        assert!(set_override("type_size").is_err());
    }
}
