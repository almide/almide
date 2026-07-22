//! Ownership certificate emission — the seam between the untrusted compiler and
//! the KERNEL-PROVEN checker (proofs/, the v1 flight-grade spine).
//!
//! `ownership_certificate` projects a function's MIR ownership ops to the
//! per-object refcount-event stream (certificate format v0): one line per
//! reference-counted OBJECT, `i` = an ownership +1 (Alloc/Dup), `d` = a −1
//! (Drop/Consume, and the move-out of a heap return). This is the SAME
//! per-object accounting [`crate::verify_ownership`] enforces — but emitted as a
//! portable certificate the proven Coq checker `check_all` re-verifies. So each
//! build's memory-safety is re-checkable by a proven artifact, not just by the
//! (untrusted) compiler's own pass.
//!
//! By construction the proven checker accepts `ownership_certificate(f)` iff
//! `verify_ownership(f)` accepts (same invariant); the unit tests pin that
//! correspondence, and `proofs/gate.sh` runs the actual proven binary on it.

use crate::{CallArg, Capability, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The name-totality witness (proofs/NameTotality.v, the 2nd flight-grade
/// property): the DEFINED value ids (params + op results) and the USED value ids
/// (operands/args). The kernel-proven `check_names` accepts iff `used ⊆ defined`
/// — i.e. no dangling MIR reference (a use of an undefined value = undefined
/// behavior). Emitted like the ownership certificate, for the proven checker.
pub struct NameWitness {
    pub defined: Vec<ValueId>,
    pub used: Vec<ValueId>,
}

/// Serialize the name-totality witness in the format `proofs/NameTotality.v`'s
/// `check_names_cert` parses: `<defined ids>|<used ids>` (space-separated nats).
/// The proven checker accepts iff `used ⊆ defined` (no dangling reference).
pub fn name_witness_string(func: &MirFunction) -> String {
    let w = name_witness(func);
    let ids = |v: &[ValueId]| v.iter().map(|x| x.0.to_string()).collect::<Vec<_>>().join(" ");
    format!("{}|{}", ids(&w.defined), ids(&w.used))
}

/// Collect the (defined, used) value ids of a function for name-totality.
/// Duplicates are harmless — the proven checker is set-membership.
pub fn name_witness(func: &MirFunction) -> NameWitness {
    let mut defined: Vec<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut used: Vec<ValueId> = Vec::new();
    let record_args = |args: &[CallArg], used: &mut Vec<ValueId>| {
        for a in args {
            if let CallArg::Handle(v) | CallArg::Scalar(v) = a {
                used.push(*v);
            }
        }
    };
    for op in &func.ops {
        match op {
            Op::Alloc { dst, .. } | Op::Const { dst } | Op::ConstInt { dst, .. } => {
                defined.push(*dst)
            }
            Op::Dup { dst, src } => {
                defined.push(*dst);
                used.push(*src);
            }
            Op::Drop { v }
            | Op::DropListStr { v }
            | Op::DropValue { v }
            | Op::DropListValue { v }
            | Op::DropListStrValue { v }
            | Op::DropListStrStr { v }
            | Op::DropListIntStr { v }
            | Op::DropListStrInt { v }
            | Op::DropResultListValue { v }
            | Op::DropResultValue { v }
            | Op::DropResultStrInt { v }
            | Op::DropResultValueInt { v }
            | Op::DropResultListValueInt { v }
            | Op::DropResultListStrInt { v }
            | Op::DropResultListStr { v }
            | Op::DropListListStr { v }
            | Op::DropVariant { v, .. }
            | Op::DropWrapperRec { v, .. }
            | Op::Consume { v }
            | Op::Borrow { v }
            | Op::MakeUnique { v } => {
                used.push(*v)
            }
            Op::Pure { dst, uses } => {
                defined.push(*dst);
                used.extend(uses.iter().copied());
            }
            Op::IntBinOp { dst, a, b, .. } => {
                defined.push(*dst);
                used.push(*a);
                used.push(*b);
            }
            Op::Prim { dst, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                used.extend(args.iter().copied());
            }
            Op::Call { dst, args, .. }
            | Op::CallFn { dst, args, .. }
            // A CallImport defines its result and USES its (borrowed/scalar) args, exactly
            // like a CallFn — its name is the host import, resolved structurally by the render.
            | Op::CallImport { dst, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                record_args(args, &mut used);
            }
            // A closure call USES the table-index value (the closure) plus its args.
            Op::CallIndirect { dst, table_idx, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                used.push(*table_idx);
                record_args(args, &mut used);
            }
            // The if-condition is USED; the result `dst` is DEFINED; the arm values are
            // USED. (The arm OPS, flat between the markers, define/use as usual.)
            Op::IfThen { cond, dst } => {
                used.push(*cond);
                if let Some(d) = dst {
                    defined.push(*d);
                }
            }
            Op::Else { val } | Op::EndIf { val } => {
                used.extend(val.iter().copied());
            }
            // Loop markers: the break cond is USED. `LoopStart`/`LoopEnd` bind nothing.
            Op::LoopBreakUnless { cond } => used.push(*cond),
            Op::LoopStart | Op::LoopEnd => {}
            // A scalar reassignment USES the source value and the target local (already
            // defined by its `var` bind — re-written, not newly defined).
            Op::SetLocal { local, src } => {
                used.push(*local);
                used.push(*src);
            }
            // A function reference DEFINES its scalar slot value; it uses no MIR value
            // (the referenced function name is resolved structurally by the render).
            Op::FuncRef { dst, .. } => defined.push(*dst),
            // The rung-4 list ops: a literal DEFINES its fresh list and USES the
            // element values; get/set USE the (borrowed) list handle + operands.
            Op::ListLit { dst, elems } => {
                defined.push(*dst);
                used.extend(elems.iter().copied());
            }
            Op::ListGetScalar { dst, list, idx } => {
                defined.push(*dst);
                used.push(*list);
                used.push(*idx);
            }
            Op::ListSetScalar { list, idx, val } => {
                used.push(*list);
                used.push(*idx);
                used.push(*val);
            }
        }
    }
    if let Some(r) = func.ret {
        used.push(r);
    }
    NameWitness { defined, used }
}

/// The capability-bound witness (proofs/CapabilityBound.v, the 4th flight-grade
/// property): the DECLARED capability allowlist (the function's effect
/// signature) and the USED capabilities (those its body's runtime calls reach).
/// The kernel-proven `check_caps` accepts iff `used ⊆ allowed` — i.e. the
/// function reaches no host effect it did not declare (the sandbox promise).
pub struct CapWitness {
    pub allowed: Vec<Capability>,
    pub used: Vec<Capability>,
}

/// Collect the (declared, used) capabilities of a function. Used capabilities
/// are derived from the runtime calls in the body via [`crate::RtFn::capability`]
/// (the single, exhaustive mapping). NOTE: capabilities reached transitively
/// through [`Op::CallFn`] (user/runtime callees) are a later brick — this
/// witness covers a function's DIRECT host effects.
/// The lifted-function NAME a value denotes, if it was bound by an `Op::FuncRef` in this
/// function — the closures caps fold reads this to follow a `CallIndirect` through a known
/// lambda (MIR values are single-assignment, so the lookup is unambiguous).
fn funcref_name(func: &MirFunction, v: ValueId) -> Option<&str> {
    func.ops.iter().find_map(|op| match op {
        Op::FuncRef { dst, name } if *dst == v => Some(name.as_str()),
        _ => None,
    })
}

pub fn cap_witness(func: &MirFunction) -> CapWitness {
    // Fold-with-independent-accumulator-writes split (codopsy8 complexity sweep): each op is
    // visited by 3 groups of INDEPENDENT `if let` checks (none of them are alternative
    // branches of one `match` — the original code already ran them as a top-to-bottom
    // sequence of separate `if let`s on the SAME `op`, so this is a pure text-move of that
    // existing structure into named helpers, no logic change, no exhaustiveness guarantee
    // lost — these were never an exhaustive `match Op { .. }` to begin with).
    let mut used: Vec<Capability> = Vec::new();
    for op in &func.ops {
        cap_witness_op_call(op, &mut used);
        cap_witness_op_prim_floor(op, &mut used);
        cap_witness_op_call_indirect(func, op, &mut used);
    }
    CapWitness { allowed: func.declared_caps.clone(), used }
}

/// Extracted from `cap_witness` (codopsy8 complexity sweep, group 1 of 3): a direct runtime
/// `Op::Call` that reaches a capability-bearing intrinsic. Verbatim.
fn cap_witness_op_call(op: &Op, used: &mut Vec<Capability>) {
    if let Op::Call { func: rt, .. } = op {
        if let Some(cap) = rt.capability() {
            used.push(cap);
        }
    }
}

/// Extracted from `cap_witness` (codopsy8 complexity sweep, group 2 of 3): the host-effect
/// FLOOR primitives — each independently gates its capability; a self-hosted runtime fn
/// using one of these prims must declare the matching capability (the `reachable_caps`
/// transitive fold then carries it to every caller through the CallFn edge into the
/// self-host body). Verbatim.
fn cap_witness_op_prim_floor(op: &Op, used: &mut Vec<Capability>) {
    // The `fd_write` primitive is the host-effect floor op — it reaches Stdout, so
    // a self-hosted runtime fn using it (print_str) must declare Stdout, exactly
    // like a `PrintStr` runtime call (this keeps the sandbox accounting complete).
    if let Op::Prim { kind: crate::PrimKind::FdWrite, .. } = op {
        used.push(Capability::Stdout);
    }
    // The `random_get` primitive is the ENTROPY floor op — reached by the self-hosted
    // `random.int`, so a fn using it must declare Entropy (the same accounting as FdWrite →
    // Stdout). The transitive `reachable_caps` follows the CallFn edge into `random.int`, so a
    // caller (pkcs1v15_pad) inherits this Entropy and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::RandomGet, .. } = op {
        used.push(Capability::Entropy);
    }
    // The `args_get_list` primitive is the CLI-ARGS floor op — reached by the self-hosted
    // `env.args`, so a fn using it must declare CliArgs (the same accounting as RandomGet →
    // Entropy). The transitive `reachable_caps` follows the CallFn edge into `env.args`, so a
    // caller inherits this CliArgs and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::ArgsGetList | crate::PrimKind::ArgsGetListFull, .. } = op {
        used.push(Capability::CliArgs);
    }
    // The `env_get` primitive is the ENVIRON floor op — reached by the self-hosted
    // `env.get`, so a fn using it must declare the Env profile's CliArgs (argv and
    // environ are the same process-initial-state class; the profile map already
    // binds `"Env" => CliArgs`). Transitive exactly like ArgsGetList.
    if let Op::Prim { kind: crate::PrimKind::EnvGet, .. } = op {
        used.push(Capability::CliArgs);
    }
    // The `read_text_file` primitive is the FS-READ floor op — reached by the self-hosted
    // `fs.read_text`, so a fn using it must declare FsRead (the same accounting as ArgsGetList →
    // CliArgs). The transitive `reachable_caps` follows the CallFn edge into `fs.read_text`, so a
    // caller inherits this FsRead and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::ReadTextFile, .. } = op {
        used.push(Capability::FsRead);
    }
    // The `read_dir` primitive is the FS-READ floor op for directory listing — reached by
    // the self-hosted `fs.list_dir`, so a fn using it must declare FsRead (the SAME
    // accounting as ReadTextFile → FsRead; both are filesystem reads). The transitive
    // `reachable_caps` follows the CallFn edge into `fs.list_dir`, so a caller inherits this
    // FsRead and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::ReadDir, .. } = op {
        used.push(Capability::FsRead);
    }
    // The `path_exists` primitive is the FS-READ floor op for an existence stat — reached by
    // the self-hosted `fs.exists`. A stat IS a filesystem read, so it REUSES Capability::FsRead
    // (NOT a new capability — the SAME accounting as ReadTextFile → FsRead). The transitive
    // `reachable_caps` follows the CallFn edge into `fs.exists`, so a caller inherits this
    // FsRead and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::PathExists, .. } = op {
        used.push(Capability::FsRead);
    }
    // The `path_filestat` primitive is the FULL-stat FS-READ floor op — reached by the
    // self-hosted `fs.stat`. A stat IS a filesystem read, so it REUSES Capability::FsRead
    // (the SAME accounting as PathExists); counted transitively through the CallFn edge
    // into `fs.stat`, so a caller is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::PathFilestat, .. } = op {
        used.push(Capability::FsRead);
    }
    // The `write_text_file` primitive is the FS-WRITE floor op — reached by the self-hosted
    // `fs.write`, so a fn using it must declare FsWrite (a DISTINCT capability from FsRead — a
    // write is strictly greater authority; the same accounting as ReadTextFile → FsRead). The
    // transitive `reachable_caps` follows the CallFn edge into `fs.write`, so a caller inherits
    // this FsWrite and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::WriteTextFile, .. } = op {
        used.push(Capability::FsWrite);
    }
    // The `make_dir` primitive is ALSO an FS-WRITE floor op — reached by the self-hosted
    // `fs.mkdir_p`. A mkdir IS a filesystem write, so it REUSES Capability::FsWrite (NOT a
    // new capability — the SAME accounting as WriteTextFile → FsWrite). The transitive
    // `reachable_caps` follows the CallFn edge into `fs.mkdir_p`, so a caller inherits this
    // FsWrite and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::MakeDir, .. } = op {
        used.push(Capability::FsWrite);
    }
    // The `clock_time_get` primitive is the WALL-CLOCK floor op — reached by the self-hosted
    // `env.unix_timestamp`, so a fn using it must declare Clock (a DISTINCT capability: a
    // clock read is neither a filesystem nor an entropy effect; the same accounting as
    // RandomGet → Entropy). The transitive `reachable_caps` follows the CallFn edge into
    // `env.unix_timestamp`, so a caller inherits this Clock and is caps-verified against its
    // declared bound.
    if let Op::Prim { kind: crate::PrimKind::ClockTimeGet, .. } = op {
        used.push(Capability::Clock);
    }
    // The `remove_all` primitive is ALSO an FS-WRITE floor op — reached by the self-hosted
    // `fs.remove_all`. A recursive remove IS a filesystem write, so it REUSES
    // Capability::FsWrite (NOT a new capability — the SAME accounting as WriteTextFile →
    // FsWrite). The transitive `reachable_caps` follows the CallFn edge into `fs.remove_all`,
    // so a caller inherits this FsWrite and is caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::RemoveAll, .. } = op {
        used.push(Capability::FsWrite);
    }
    // The `read_line` primitive is the STANDARD-INPUT floor op — reached by the self-hosted
    // `io.read_line`, so a fn using it must declare Stdin (a DISTINCT capability: reading the
    // operator's input stream is neither a write, a filesystem, an entropy, nor a clock
    // effect; the same accounting as RandomGet → Entropy). The transitive `reachable_caps`
    // follows the CallFn edge into `io.read_line`, so a caller inherits this Stdin and is
    // caps-verified against its declared bound.
    if let Op::Prim { kind: crate::PrimKind::ReadLine | crate::PrimKind::ReadNBytes, .. } = op {
        used.push(Capability::Stdin);
    }
}

/// Extracted from `cap_witness` (codopsy8 complexity sweep, group 3 of 3): SOUNDNESS CRUX — a
/// CallIndirect invokes a closure that may reach ANY capability. When the table index
/// resolves to a KNOWN lifted lambda (a `FuncRef` in THIS function), its REAL caps are
/// folded transitively by `reachable_caps` (which follows the same `FuncRef` edge) — no
/// conservative taint needed, so a non-printing closure stays caps-verified. Only a DYNAMIC
/// closure (table_idx not a local `FuncRef` — e.g. a closure PARAMETER) is unanalyzable
/// here, so it conservatively marks Stdout used: such a fn is caps-verified ONLY if it
/// DECLARES it (a closure that secretly writes Stdout can never pass un-witnessed —
/// accept-but-unsafe). Verbatim.
fn cap_witness_op_call_indirect(func: &MirFunction, op: &Op, used: &mut Vec<Capability>) {
    if let Op::CallIndirect { table_idx, .. } = op {
        if funcref_name(func, *table_idx).is_none() {
            used.push(Capability::Stdout);
        }
    }
}

/// Serialize the capability witness in the format `proofs/CapabilityBound.v`'s
/// `check_caps_cert` parses: `<allowed ids>|<used ids>` (space-separated
/// registry ids, via [`Capability::id`]). The proven checker accepts iff
/// `used ⊆ allowed` (no undeclared host effect).
pub fn cap_witness_string(func: &MirFunction) -> String {
    let w = cap_witness(func);
    let ids = |v: &[Capability]| {
        v.iter().map(|c| c.id().to_string()).collect::<Vec<_>>().join(" ")
    };
    format!("{}|{}", ids(&w.allowed), ids(&w.used))
}

/// The capabilities a function reaches TRANSITIVELY: its direct caps (its own
/// runtime calls) plus those of every function it calls via [`Op::CallFn`], to a
/// fixpoint. `program` maps a function name to its MIR; `visited` breaks cycles.
/// This is the COMPILER-side reachability fold — the proven checker re-verifies
/// the result by the per-call-site subset rule (`check_caps`), so a program is
/// rejected for a capability a CALLEE reaches even with no direct effect.
///
/// NOTE (honest scope): a callee NOT in `program` (out of the lowering subset)
/// contributes no caps here — sound only when every reachable function lowers;
/// treating an unknown callee as reaching ANY capability (conservative reject)
/// is the hardening that makes it sound in general.
pub fn reachable_caps(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    visited: &mut std::collections::BTreeSet<String>,
) -> Vec<Capability> {
    let mut caps: Vec<Capability> = Vec::new();
    if !visited.insert(name.to_string()) {
        return caps; // already folded in (cycle / diamond)
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return caps,
    };
    caps.extend(cap_witness(func).used); // direct caps
    for op in &func.ops {
        if let Op::CallFn { name: callee, .. } = op {
            caps.extend(reachable_caps(callee, program, visited));
        }
        // A FuncRef CREATES a closure to a known lifted lambda; fold that lambda's caps at
        // CREATION. This accounts the closure's effects in this function regardless of HOW
        // or WHETHER it is later invoked (a CallIndirect, a deferred call, an operand call,
        // or never) — so there is NO call-site coverage requirement, which is what makes
        // incremental lambda-lifting sound. Precise: a pure lambda folds ∅, a printing one
        // folds Stdout. The same edge cap_witness trusts to drop the CallIndirect taint.
        if let Op::FuncRef { name: callee, .. } = op {
            caps.extend(reachable_caps(callee, program, visited));
        }
    }
    caps
}

/// The TRANSITIVE capability witness for a caller: `<declared ids>|<reachable
/// ids>` (reachable = direct ∪ all callees' caps, transitively). The proven
/// `check_caps_cert` accepts iff `reachable ⊆ declared` — the per-call-site
/// subset rule applied across the call graph, with the checker doing only the
/// subset (the compiler did the reachability fold).
pub fn transitive_cap_witness_string(
    func: &MirFunction,
    program: &BTreeMap<String, MirFunction>,
) -> String {
    let mut visited = std::collections::BTreeSet::new();
    let reachable = reachable_caps(func.name.as_str(), program, &mut visited);
    let ids = |v: &[Capability]| {
        v.iter().map(|c| c.id().to_string()).collect::<Vec<_>>().join(" ")
    };
    format!("{}|{}", ids(&func.declared_caps), ids(&reachable))
}

/// A capability id NO real [`Capability::id`] emits (the registry is Stdout=0 today). The
/// sentinel UNIVERSE node of the call-graph witness directly "reaches" it, so any function
/// that reaches an unanalyzable callee (routed to UNIVERSE) is rejected by the proven checker.
const SENTINEL_CAP: u32 = 1_000_000;

/// Emit the TRANSITIVE capability witness as a CALL GRAPH for the kernel-proven
/// `check_prog_cert` (proofs/CapabilityReach.v): functions `;`-separated, each
/// `<declared ids>|<direct ids>|<callee indices>`, callees as 0-based indices into the
/// emitted (sorted-by-name) order. Unlike [`transitive_cap_witness_string`] — which makes the
/// COMPILER fold reachability and emits the result — this emits only the GRAPH, and the proven
/// checker COMPUTES the transitive reach and checks `reach ⊆ declared` per function. The
/// reachability fold thus moves OUT of the untrusted compiler INTO the proof.
///
/// An unknown / cross-file / `is_elided` callee (whose effects this graph cannot see) is
/// pointed at a sentinel UNIVERSE node (appended last) that directly reaches [`SENTINEL_CAP`],
/// a capability no real function declares — so any caller transitively reaching it is REJECTED.
/// This is the same conservative direction as [`reaches_capability_or_unknown`], now decided by
/// the proof. UNIVERSE declares the sentinel itself, so it passes its own `reach ⊆ declared`.
pub fn program_cap_graph_witness(
    program: &BTreeMap<String, MirFunction>,
    is_known_free: &dyn Fn(&str) -> bool,
    is_elided: &dyn Fn(&str) -> bool,
) -> String {
    let names: Vec<&str> = program.keys().map(|s| s.as_str()).collect();
    let index_of: BTreeMap<&str, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let universe = names.len();
    let ids = |v: &[Capability]| {
        v.iter().map(|c| c.id().to_string()).collect::<Vec<_>>().join(" ")
    };
    let mut fns: Vec<String> = Vec::with_capacity(names.len() + 1);
    for name in &names {
        let func = &program[*name];
        let mut callees: Vec<usize> = Vec::new();
        if is_elided(name) {
            callees.push(universe); // an elided call hides effects from the graph — taint
        }
        for op in &func.ops {
            let callee = match op {
                Op::CallFn { name: c, .. } | Op::FuncRef { name: c, .. } => Some(c.as_str()),
                _ => None,
            };
            if let Some(c) = callee {
                if let Some(&idx) = index_of.get(c) {
                    callees.push(idx);
                } else if !is_known_free(c) {
                    callees.push(universe); // unknown / cross-file callee — conservatively taint
                }
                // a known-effect-free out-of-program callee reaches nothing → omit
            }
        }
        let cs = callees.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        fns.push(format!("{}|{}|{}", ids(&func.declared_caps), ids(&cap_witness(func).used), cs));
    }
    // UNIVERSE: declares + directly uses the sentinel (so it passes its own check) and has no
    // callees, so it taints any function reaching it without polluting the real graph.
    fns.push(format!("{}|{}|", SENTINEL_CAP, SENTINEL_CAP));
    fns.join(";")
}

/// Project the `almide.toml [permissions].allow` MANIFEST vocabulary (the
/// production permission strings `cli::check_permissions` consumes: `IO` /
/// `Rand` / `Env` / `Time` / …) onto the MIR [`Capability`] registry. This is
/// the DECLARED side of the capability witness when a manifest exists — the
/// operator's written bound, not the vacuous effect-fn-declares-everything
/// default. Effects with no modeled MIR capability yet (`Net`, `Fan`) project
/// to nothing: they cannot silently widen the bound.
pub fn manifest_caps(allow: &[String]) -> Vec<Capability> {
    let mut caps: Vec<Capability> = Vec::new();
    for p in allow {
        match p.as_str() {
            "IO" => caps.extend([
                Capability::Stdout,
                Capability::Stdin,
                Capability::FsRead,
                Capability::FsWrite,
            ]),
            "Rand" => caps.push(Capability::Entropy),
            "Env" => caps.push(Capability::CliArgs),
            "Time" => caps.push(Capability::Clock),
            _ => {}
        }
    }
    caps
}

/// Refine an EFFECT function's declared capability bound to the manifest
/// (`[permissions].allow` → `effect fn` declares ONLY those capabilities — the
/// roadmap's Phase-1 semantics on the witness path). A pure `fn` keeps ∅: a
/// manifest can never GRANT host access the effect system denies (the
/// pure-stays-pure soundness floor is untouched). Before this refinement an
/// `effect fn` declared every modeled capability, so `used ⊆ declared` could
/// never REJECT it — the manifest makes the sandbox promise non-vacuous.
pub fn apply_manifest_caps(func: &mut MirFunction, allow: &[String]) {
    if !func.declared_caps.is_empty() {
        func.declared_caps = manifest_caps(allow);
    }
}

/// The mode nat of a heap param / heap call arg in the CALL-MODE witness
/// (proofs/CallModes.v: 0 = borrow, 1 = move). The v1 calling convention is
/// borrow-only today ([`MirParam`]: the caller keeps its reference, the callee's
/// cert seeds the param at 0; [`CallArg::Handle`]: live-checked, refcount
/// unchanged) — so every emitted mode is `0`. When the lowering gains consuming
/// (move) params, it flips the mode HERE and in the signature TOGETHER or the
/// proven checker rejects the build: that agreement is the point of the witness.
const MODE_BORROW: u32 = 0;

/// Emit the CALL-MODE SIGNATURE witness for the kernel-proven `check_modes_cert`
/// (proofs/CallModes.v, certificate format v1 brick 2c): `<sigs>|<sites>` —
/// signatures `;`-separated in the emitted (sorted-by-name) function order, each
/// the space-separated heap-param modes of that function; call sites
/// `;`-separated, each `<callee index> <actual modes…>`. The proven checker
/// accepts iff every site's actual modes EQUAL its callee's declared signature —
/// the ground fact that makes per-function ownership certs COMPOSE
/// (`CallModes.check_fill_sound`): a callee that assumed move while its caller
/// assumed borrow (both per-function-balanced, inlined = double-free) can no
/// longer slip through.
///
/// Honest scope: [`Op::CallFn`] sites, plus [`Op::CallIndirect`] sites via the
/// POSSIBLE-CALLEE set (brick 5c). A runtime [`Op::Call`] follows the fixed
/// borrow-args/owned-result convention (a renderer contract, not a per-function
/// signature). An out-of-program callee with a KNOWN calling convention
/// (`is_known_convention` — the caller's policy, e.g. dotted self-hosted stdlib
/// names, purity-gated at lowering and borrowing their heap args by the same
/// renderer contract as [`Op::Call`]) is omitted; any OTHER unknown /
/// cross-file callee gets an out-of-range index, which the checker REJECTS
/// (conservative, same discipline as the caps graph's universe node).
///
/// CLOSURE SIGNATURES (brick 5c): a [`Op::CallIndirect`] dispatches through a
/// funcref whose runtime target the caller cannot name — but every funcref
/// VALUE originates from some [`Op::FuncRef`] in the program (the closure-table
/// ground truth, the same fact the caps fold uses at FuncRef creation). The
/// site's possible-callee set = the FuncRef targets whose param SHAPE matches
/// the site (arity + per-position heapness — the `call_indirect` type gate
/// traps on any other target, fail-stop). The witness emits ONE agreement row
/// PER POSSIBLE CALLEE — `forallb site_ok` in the proven checker then IS the
/// "agreement against every member of the set" Forall lift, with no new Coq
/// surface. A site whose set is EMPTY, or reaching a FuncRef target that never
/// lowered (its signature unseeable), emits the out-of-range sentinel —
/// conservative REJECT, never a silent skip.
pub fn call_modes_witness(
    program: &BTreeMap<String, MirFunction>,
    is_known_convention: &dyn Fn(&str) -> bool,
) -> String {
    // Sequential-phase split (codopsy8 complexity sweep, helpers in certificate_c.rs to keep
    // this file under the max-lines threshold): the 3 phases below each build ONE
    // independent output collection; a later phase only READS an earlier phase's finished
    // output (never interleaves writes) — the exact "linear waterfall" shape round7/round8
    // established as safe to decompose (unlike a true state-threading rollback). Pure
    // text-move, no logic change.
    let names: Vec<&str> = program.keys().map(|s| s.as_str()).collect();
    let index_of: BTreeMap<&str, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let unknown = names.len(); // out of range — the checker rejects any site naming it
    let sigs = call_modes_witness_sigs(program, &names);
    let (table_targets, table_unseeable) = call_modes_witness_func_table(program, &names);
    let sites = call_modes_witness_sites(
        program,
        &names,
        &index_of,
        unknown,
        is_known_convention,
        &table_targets,
        table_unseeable,
    );
    format!("{}|{}", sigs.join(";"), sites.join(";"))
}

/// Conservative transitive capability-reachability — the SOUND basis for a
/// corpus capability gate across `Op::CallFn` edges. A function's empty (direct)
/// capability witness is a sound claim of effect-freedom ONLY if this returns
/// `false`: the direct witness alone misses what a CALLEE reaches, and
/// [`reachable_caps`] treats an unknown callee as contributing ∅ — unsound for an
/// effectful one (its honest-scope caveat). This closes that hole conservatively.
///
/// Returns `true` if `name` reaches a host capability DIRECTLY (it has an
/// `Op::Call` whose `RtFn` bears one) or through ANY `Op::CallFn` callee that is
/// not provably effect-free. A callee NOT in `program` is provably free only when
/// `is_known_free(callee)` — the CALLER supplies that policy (e.g. variant
/// constructors, known effect-free builtins, purity-gated stdlib `Module` calls).
/// Any other unknown callee (a walled or cross-file user function whose effects
/// are unseen) is treated as reaching a capability — the conservative direction,
/// so a gate built on this NEVER over-accepts. `visited` breaks cycles.
///
/// `is_elided(name)` reports a function whose source had MORE call nodes than its
/// MIR has call-ops — i.e. a call ELIDED by Opaque lowering (a list element, a
/// ctor payload, a BinOp operand). An elided call's effects are absent from
/// `func.ops`, so this fold cannot see them; such a function (and so any caller)
/// is conservatively TAINTED — its capability witness is incompletely captured
/// and must not be claimed safe.
pub fn reaches_capability_or_unknown(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    is_known_free: &dyn Fn(&str) -> bool,
    is_elided: &dyn Fn(&str) -> bool,
    visited: &mut std::collections::BTreeSet<String>,
) -> bool {
    if !visited.insert(name.to_string()) {
        return false; // cycle / diamond: already accounted on the stack
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return !is_known_free(name),
    };
    if is_elided(name) {
        return true; // an elided call hides effects from this fold — conservatively tainted
    }
    if !cap_witness(func).used.is_empty() {
        return true; // a direct host effect (today: Stdout via an RtFn `Op::Call`)
    }
    func.ops.iter().any(|op| match op {
        Op::CallFn { name: callee, .. } => {
            reaches_capability_or_unknown(callee, program, is_known_free, is_elided, visited)
        }
        // A FuncRef closure's effects reach this function at creation — fold like a callee
        // (the boolean counterpart of the FuncRef edge in `reachable_caps_or_tainted`).
        Op::FuncRef { name: callee, .. } => {
            reaches_capability_or_unknown(callee, program, is_known_free, is_elided, visited)
        }
        _ => false,
    })
}

/// The transitive reachable capabilities of `name`, or `None` if its `Op::CallFn`
/// closure hits an UNANALYZABLE callee — an unknown/cross-file callee (not in
/// `program` and not `is_known_free`) or one with an ELIDED call (`is_elided`)
/// whose effects are absent from its MIR. A `None` function cannot be capability-
/// verified (its reachable set is incomplete, so a hidden effect could exceed any
/// declared bound). A `Some(set)` function's effects are FULLY known: the gate
/// then emits `<declared>|<set>` and the proven `check_caps_cert` verifies
/// `set ⊆ declared` — so an EFFECTFUL function is verified against its OWN declared
/// capability bound, not merely excluded for touching a capability. This is the
/// set-valued counterpart of [`reaches_capability_or_unknown`].
pub fn reachable_caps_or_tainted(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    is_known_free: &dyn Fn(&str) -> bool,
    is_elided: &dyn Fn(&str) -> bool,
    visited: &mut std::collections::BTreeSet<String>,
) -> Option<Vec<Capability>> {
    if !visited.insert(name.to_string()) {
        return Some(Vec::new()); // cycle / diamond: already folded on the stack
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return if is_known_free(name) { Some(Vec::new()) } else { None },
    };
    if is_elided(name) {
        return None; // an elided call hides effects from this fold — unanalyzable
    }
    let mut caps = cap_witness(func).used;
    for op in &func.ops {
        if let Op::CallFn { name: callee, .. } = op {
            match reachable_caps_or_tainted(callee, program, is_known_free, is_elided, visited) {
                Some(c) => caps.extend(c),
                None => return None,
            }
        }
        // A FuncRef CREATES a closure to a lifted lambda — fold its caps at CREATION,
        // exactly as [`reachable_caps`] does. Coverage-free: the closure's effects reach
        // this function however or whether it is later invoked (a CallIndirect, a deferred
        // call, or never). WITHOUT this, a function holding a printing lifted lambda would
        // be falsely caps-VERIFIED here (the lambda's Stdout unseen by the CallFn-only fold)
        // the moment lambda-lifting emits FuncRef into the corpus — an accept-but-unsafe
        // hole. The lambda is in `program` (the harness puts every lifted aux in the
        // in-profile map); an unanalyzable/elided lambda taints (`None`) like any callee.
        if let Op::FuncRef { name: callee, .. } = op {
            match reachable_caps_or_tainted(callee, program, is_known_free, is_elided, visited) {
                Some(c) => caps.extend(c),
                None => return None,
            }
        }
    }
    Some(caps)
}

/// One open `IfThen…Else…EndIf` region (format v4, brick 5a): per-object events
/// collected PER ARM, so the flush at `EndIf` can decide — arms that each
/// self-balance (net 0) for the object flush FLAT (byte-identical to the
/// ungrouped emission: zero churn on every existing cert), arms that do NOT are
/// grouped as `{then|else}` and the proven checker re-derives their AGREEMENT
/// from the arm bodies themselves (`CBranch`). Cross-arm compensation — an `i`
/// in one arm balanced by a `d` in the other, runtime-unsafe whichever way the
/// branch goes yet flat-balanced — thereby becomes structurally rejected: the
/// lowering's per-arm-balance promise is no longer a TRUSTED convention.
#[derive(Default)]
struct BranchFrame {
    then_ev: BTreeMap<ValueId, String>,
    else_ev: BTreeMap<ValueId, String>,
    order: Vec<ValueId>, // objects in first-touch order (across both arms)
    in_else: bool,
}

include!("certificate_b.rs");
include!("certificate_c.rs");
