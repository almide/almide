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
    let mut used: Vec<Capability> = Vec::new();
    for op in &func.ops {
        if let Op::Call { func: rt, .. } = op {
            if let Some(cap) = rt.capability() {
                used.push(cap);
            }
        }
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
        // SOUNDNESS CRUX: a CallIndirect invokes a closure that may reach ANY capability.
        // When the table index resolves to a KNOWN lifted lambda (a `FuncRef` in THIS
        // function), its REAL caps are folded transitively by `reachable_caps` (which
        // follows the same `FuncRef` edge) — no conservative taint needed, so a non-printing
        // closure stays caps-verified. Only a DYNAMIC closure (table_idx not a local
        // `FuncRef` — e.g. a closure PARAMETER) is unanalyzable here, so it conservatively
        // marks Stdout used: such a fn is caps-verified ONLY if it DECLARES it (a closure
        // that secretly writes Stdout can never pass un-witnessed — accept-but-unsafe).
        if let Op::CallIndirect { table_idx, .. } = op {
            if funcref_name(func, *table_idx).is_none() {
                used.push(Capability::Stdout);
            }
        }
    }
    CapWitness { allowed: func.declared_caps.clone(), used }
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
    let names: Vec<&str> = program.keys().map(|s| s.as_str()).collect();
    let index_of: BTreeMap<&str, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let unknown = names.len(); // out of range — the checker rejects any site naming it
    let sigs: Vec<String> = names
        .iter()
        .map(|name| {
            program[*name]
                .params
                .iter()
                .filter(|p| p.repr.is_heap())
                .map(|_| MODE_BORROW.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    // The function TABLE: every FuncRef target anywhere in the program — the
    // over-approximation of what any CallIndirect can reach. A target that never
    // lowered (absent from `program`) poisons the table: its signature is
    // unseeable, so every indirect site becomes unknowable (sentinel).
    let mut table_targets: Vec<&str> = Vec::new();
    let mut table_unseeable = false;
    for name in &names {
        for op in &program[*name].ops {
            if let Op::FuncRef { name: target, .. } = op {
                if program.contains_key(target.as_str()) {
                    if !table_targets.contains(&target.as_str()) {
                        table_targets.push(target.as_str());
                    }
                } else {
                    table_unseeable = true;
                }
            }
        }
    }
    let mut sites: Vec<String> = Vec::new();
    for name in &names {
        for op in &program[*name].ops {
            match op {
                Op::CallFn { name: callee, args, .. } => {
                    let idx = match index_of.get(callee.as_str()) {
                        Some(&i) => i,
                        None if is_known_convention(callee) => continue, // renderer-contract callee
                        None => unknown, // unknown callee — the checker rejects the site
                    };
                    let mut site = vec![idx.to_string()];
                    site.extend(args.iter().filter_map(|a| match a {
                        CallArg::Handle(_) => Some(MODE_BORROW.to_string()),
                        CallArg::Scalar(_) | CallArg::Imm(_) | CallArg::Label(_) => None,
                    }));
                    sites.push(site.join(" "));
                }
                Op::CallIndirect { args, .. } => {
                    let actual: Vec<String> = args
                        .iter()
                        .filter_map(|a| match a {
                            CallArg::Handle(_) => Some(MODE_BORROW.to_string()),
                            CallArg::Scalar(_) | CallArg::Imm(_) | CallArg::Label(_) => None,
                        })
                        .collect();
                    // Possible callees: table targets whose param shape matches the
                    // site (any other target traps at dispatch — excluded soundly).
                    let possible: Vec<usize> = if table_unseeable {
                        Vec::new()
                    } else {
                        table_targets
                            .iter()
                            .filter(|t| {
                                let f = &program[**t];
                                f.params.len() == args.len()
                                    && f.params.iter().zip(args).all(|(p, a)| {
                                        p.repr.is_heap() == matches!(a, CallArg::Handle(_))
                                    })
                            })
                            .filter_map(|t| index_of.get(*t).copied())
                            .collect()
                    };
                    if possible.is_empty() {
                        // Unknowable dispatch — the sentinel row rejects the build.
                        let mut site = vec![unknown.to_string()];
                        site.extend(actual.iter().cloned());
                        sites.push(site.join(" "));
                    } else {
                        for idx in possible {
                            let mut site = vec![idx.to_string()];
                            site.extend(actual.iter().cloned());
                            sites.push(site.join(" "));
                        }
                    }
                }
                _ => {}
            }
        }
    }
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

/// Per-object refcount-event accumulator, preserving object creation order.
struct Streams {
    of: BTreeMap<ValueId, ValueId>, // handle → object representative
    order: Vec<ValueId>,            // objects in first-seen order
    stream: BTreeMap<ValueId, String>,
    frames: Vec<BranchFrame>, // open IfThen regions, innermost last
}

fn seg_net(seg: &str) -> i64 {
    seg.chars()
        .map(|c| match c {
            'i' | 'a' => 1,
            'd' | 'm' => -1,
            _ => 0, // b (+0), loop/branch delimiters
        })
        .sum()
}

impl Streams {
    fn new() -> Self {
        Streams {
            of: BTreeMap::new(),
            order: Vec::new(),
            stream: BTreeMap::new(),
            frames: Vec::new(),
        }
    }
    /// Append an event segment to `o` — into the innermost open branch arm when
    /// one exists (buffered until the region's flush), else onto the stream.
    fn append_seg(&mut self, o: ValueId, seg: &str) {
        if let Some(fr) = self.frames.last_mut() {
            if !fr.then_ev.contains_key(&o) && !fr.else_ev.contains_key(&o) {
                fr.order.push(o);
            }
            let map = if fr.in_else { &mut fr.else_ev } else { &mut fr.then_ev };
            map.entry(o).or_default().push_str(seg);
            return;
        }
        if !self.stream.contains_key(&o) {
            self.stream.insert(o, String::new());
            self.order.push(o);
        }
        self.stream.get_mut(&o).unwrap().push_str(seg);
    }
    /// Record a +1/−1/+0 event (`'i'`/`'d'`/`'b'`…) on object `o`.
    fn event(&mut self, o: ValueId, c: char) {
        let mut buf = [0u8; 4];
        self.append_seg(o, c.encode_utf8(&mut buf));
    }
    /// Open an `IfThen` region: subsequent events buffer into its then arm.
    fn open_branch(&mut self) {
        self.frames.push(BranchFrame::default());
    }
    /// `Else` marker: subsequent events buffer into the else arm.
    fn else_branch(&mut self) {
        if let Some(fr) = self.frames.last_mut() {
            fr.in_else = true;
        }
    }
    /// Close the innermost region (`EndIf`): per object, flush FLAT when both
    /// arms self-balance (net 0 — byte-identical to the ungrouped emission),
    /// else grouped `{then|else}` (the proven CBranch agreement rule). An arm
    /// that itself contains a region delimiter (a nested grouped branch or a
    /// loop) cannot be represented in a FLAT v4 arm body — emit the always-
    /// rejecting poison `{i|}` instead (conservative: never a silent accept).
    fn flush_branch(&mut self) {
        let fr = match self.frames.pop() {
            Some(fr) => fr,
            None => return, // EndIf without IfThen — malformed MIR, nothing buffered
        };
        for o in fr.order {
            let t = fr.then_ev.get(&o).cloned().unwrap_or_default();
            let e = fr.else_ev.get(&o).cloned().unwrap_or_default();
            let seg = if seg_net(&t) == 0 && seg_net(&e) == 0 {
                format!("{t}{e}")
            } else if t.contains(['(', ')', '{', '}', '[', ']'])
                || e.contains(['(', ')', '{', '}', '[', ']'])
            {
                "{i|}".to_string()
            } else {
                format!("{{{t}|{e}}}")
            };
            self.append_seg(o, &seg);
        }
    }
    /// The current rc balance of `o`'s line (i/a = +1, d/m = −1), INCLUDING the
    /// events buffered in open branch arms — used to decide whether a
    /// branch-merge val still HOLDS its reference (an un-consumed arm value
    /// flowing through `EndIf {{ val }}` is a real move the stream must see).
    fn balance(&self, o: ValueId) -> i64 {
        let mut b = self.stream.get(&o).map(|line| seg_net(line)).unwrap_or(0);
        for fr in &self.frames {
            if let Some(t) = fr.then_ev.get(&o) {
                b += seg_net(t);
            }
            if let Some(e) = fr.else_ev.get(&o) {
                b += seg_net(e);
            }
        }
        b
    }
    fn object_of(&self, handle: ValueId) -> ValueId {
        // Well-formed MIR always has the handle mapped; fall back to identity so a
        // malformed input yields an unbalanced (rejected) certificate rather than
        // a panic.
        self.of.get(&handle).copied().unwrap_or(handle)
    }
}

/// Pre-scan for HEAP loop-carried SLOTS (option C). A `SetLocal { local, src }`
/// inside a `LoopStart`…`LoopEnd` region, whose `src` is a heap object (an
/// `Alloc`/heap-call-result allocated in the loop body — the `acc + [x]` feeder),
/// makes `local` a loop-carried accumulator slot: across iterations the slot drops
/// its old object and acquires `src` as the new one. The certificate folds the
/// slot's per-iteration drop-old + acquire-new into ONE stream wrapped in loop
/// delimiters `(`…`)`, so it reads `i(id)m` (acquire once; loop body acquire-new +
/// drop-old = rc-preserving; move out the final) — accepted by the proven
/// `check_cert_lc`. Returns `feeder -> slot` (route the feeder's `i` to the slot
/// stream) and the set of slot locals (open/close `(`/`)` around the loop body).
fn loop_carried_slots(
    func: &MirFunction,
) -> (BTreeMap<ValueId, ValueId>, BTreeSet<ValueId>, BTreeSet<ValueId>) {
    // Heap object dsts: Alloc, and calls with a heap result.
    let mut heap_objs: BTreeSet<ValueId> = BTreeSet::new();
    for p in &func.params {
        if p.repr.is_heap() {
            heap_objs.insert(p.value);
        }
    }
    // Open-branch stack for the merge-dst scan below: (IfThen dst, then-arm val was heap).
    let mut if_stack: Vec<(Option<ValueId>, bool)> = Vec::new();
    for op in &func.ops {
        match op {
            // ListLit joins Alloc as an alloc-class introducer (rung 4/5: scalar
            // list AND record literals) — without it a record reassign's SetLocal
            // feeder goes unrecognized and the slot reads flat `idd` + `i` (the
            // exact false double-free/leak the kernel checker rejected when the
            // records slab first landed).
            Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => {
                heap_objs.insert(*dst);
            }
            Op::Call { dst: Some(d), result: Some(r), .. }
            | Op::CallFn { dst: Some(d), result: Some(r), .. }
            | Op::CallImport { dst: Some(d), result: Some(r), .. }
            | Op::CallIndirect { dst: Some(d), result: Some(r), .. }
                if r.is_heap() =>
            {
                heap_objs.insert(*d);
            }
            // A Dup is ALWAYS a heap handle (an alias acquire on an existing heap
            // object — scalars are never Dup'd): the SWAP-CARRY rebind (`cur =
            // merged` lowered as `Dup tmp = merged; Drop cur; SetLocal cur = tmp`
            // since the whole-var alias-edge elision) feeds its slot through the
            // Dup's dst. Without this the slot goes unrecognized and the in-loop
            // drop-old + scope-end drop read flat (`idd`) — the loop_buffer_churn
            // false double-free the Trust Spine gate caught. NO src gate: the
            // C-132 write-back rebind (`t = __mp_buf` in a loop) Dups a BORROWED
            // tuple-slot LoadHandle — not itself in `heap_objs` — and the src-gated
            // form left that slot unrecognized (`idm` + a flat `a`, both rejected).
            Op::Dup { dst, .. } => {
                heap_objs.insert(*dst);
            }
            // A branch-MERGE dst whose arm value is heap (`acc = if c then acc + [x]
            // else acc` — the arm's Else/EndIf val moves the arm's heap object into
            // the merge) IS a heap object: the following `SetLocal { local, src: dst }`
            // is the loop-carried rebind, and the slot goes unrecognized without this
            // (the accumulator then reads flat `iamdm` — a false imbalance the kernel
            // checker rejects while the strict render runs correctly). The stack pairs
            // each EndIf with its IfThen so nesting resolves inner-first; `Else { val }`
            // carries the then-arm value, `EndIf { val }` the else-arm value.
            Op::IfThen { dst, .. } => {
                if_stack.push((*dst, false));
            }
            Op::Else { val } => {
                if let (Some(frame), Some(v)) = (if_stack.last_mut(), val) {
                    if heap_objs.contains(v) {
                        frame.1 = true;
                    }
                }
            }
            Op::EndIf { val } => {
                if let Some((dst, then_heap)) = if_stack.pop() {
                    let heap = then_heap || val.map_or(false, |v| heap_objs.contains(&v));
                    if heap {
                        if let Some(d) = dst {
                            heap_objs.insert(d);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut feeder_to_slot: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut slots: BTreeSet<ValueId> = BTreeSet::new();
    // STRAIGHT-LINE (non-loop) heap slots: a `SetLocal { local, src }` with a heap `src` OUTSIDE any
    // loop region (the unrolled identity-else shadow-rebind append-accumulator — porta serialize_opts).
    // Each such reassign is folded into its OWN `(id)` CLoop body (`(` at the feeder's `i`, `)` at the
    // SetLocal), so a body with k reassigns reads `i(id)…(id)m` — the SAME rc-preserving unit the loop
    // slot proves, accepted by check_cert_lc. A SCALAR `src` (a loop counter `i+1`) is not a heap_obj,
    // so it is never a slot here (no spurious fold).
    let mut line_slots: BTreeSet<ValueId> = BTreeSet::new();
    let mut depth: u32 = 0;
    for op in &func.ops {
        match op {
            Op::LoopStart => depth += 1,
            Op::LoopEnd => depth = depth.saturating_sub(1),
            Op::SetLocal { local, src } if heap_objs.contains(src) => {
                feeder_to_slot.insert(*src, *local);
                if depth > 0 {
                    slots.insert(*local);
                } else {
                    line_slots.insert(*local);
                }
            }
            _ => {}
        }
    }
    (feeder_to_slot, slots, line_slots)
}

/// Emit the per-object ownership certificate (format v2) for a function. Heap
/// loop-carried accumulator slots are folded into a single `i(id)m` stream with
/// loop delimiters (option C); everything else is the flat per-object format.
/// The mutable emission state of [`ownership_certificate`] — one step per op
/// (#781: the cog-123 loop body became [`CertScan::step`]).
struct CertScan {
    depth: u32,
    s: Streams,
    released_merge_dsts: std::collections::HashSet<crate::ValueId>,
    consumed_values: std::collections::HashSet<crate::ValueId>,
    feeder_to_slot: BTreeMap<ValueId, ValueId>,
    slots: BTreeSet<ValueId>,
    line_slots: BTreeSet<ValueId>,
}

impl CertScan {
    /// One op's certificate emission. Verbatim text move of the emission loop
    /// body (locals renamed to fields).
    fn step(&mut self, op: &Op) {
        match op {
            // A rung-4 scalar-list LITERAL is alloc-class — the IDENTICAL `i` (and
            // loop-slot feeder routing) the `Alloc{DynList}` it replaced emitted.
            // The element load/store ops are ownership-NEUTRAL (a borrowed handle
            // read/write), so they need no event arm — the catch-all below skips them.
            Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => {
                // An Alloc that FEEDS a loop-carried slot routes its `i` into the slot
                // stream (folded inside the loop delimiters); otherwise its own stream.
                if let Some(&slot) = self.feeder_to_slot.get(dst) {
                    // Resolve the slot through `of`: a Dup-INITIALIZED slot (`var iv =
                    // state.iv`) aliases the Dup'self.s source object — its 'a'/'d'/'m' land
                    // there, so the loop `(i…)`/feeder events must land on the SAME
                    // stream (they split across two unbalanced lines otherwise — the
                    // bytes_set_value_semantics::rotate REJECT, F8 residue).
                    let so = self.s.object_of(slot);
                    self.s.of.insert(*dst, so);
                    if self.line_slots.contains(&slot) {
                        self.s.event(so, '(');
                    }
                    self.s.event(so, 'i');
                } else {
                    self.s.of.insert(*dst, *dst);
                    self.s.event(*dst, 'i');
                }
            }
            Op::Dup { dst, src } => {
                // ALIAS acquire (+1): a new handle on an existing shared object.
                // `a` (not `i`) records the share-vs-move ground fact (format v1).
                // A Dup that FEEDS a loop-carried slot (`cur = merged` swap-carry:
                // `Dup tmp = merged; Drop cur; SetLocal cur = tmp`) routes its `a`
                // into the SLOT stream, exactly as the Alloc/heap-call feeders route
                // their `i`: the slot'self.s per-iteration acquire-new + drop-old then
                // reads `(ad)` (rc-preserving), instead of the drop-old landing flat
                // next to the scope-end drop (`idd` — a false double-free).
                if let Some(&slot) = self.feeder_to_slot.get(dst) {
                    let so = self.s.object_of(slot);
                    self.s.of.insert(*dst, so);
                    if self.line_slots.contains(&slot) {
                        self.s.event(so, '(');
                    }
                    self.s.event(so, 'a');
                } else {
                    let o = self.s.object_of(*src);
                    self.s.of.insert(*dst, o);
                    self.s.event(o, 'a');
                }
            }
            // Plain release (−1). A `DropListStr`/`DropListValue` is the SAME single `d` on the LIST
            // object — its elements were already accounted as `m` (consumed) when stored into it, so
            // the recursive runtime free (per-String, or per-Value via `$__drop_value`) adds no extra
            // cert event.
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
            | Op::DropWrapperRec { v, .. } => {
                let o = self.s.object_of(*v);
                self.s.event(o, 'd');
            }
            // MOVE-OUT (−1): the reference is transferred out (into a container /
            // a consuming callee). `m` distinguishes move from a plain drop.
            Op::Consume { v } => {
                let o = self.s.object_of(*v);
                self.s.event(o, 'm');
            }
            // A call that returns a FRESH OWNED heap value (the callee allocated
            // it and moved it out to us — the return-mode signature read at the
            // call site, callee not opened) is a +1, like Alloc. A `CallIndirect`
            // (a closure invocation) returning heap is the SAME: a closure moves its
            // result out, so a heap-returning closure call (`let o = f(x)` where
            // `f: (Int) -> Option[Int]`) owns a fresh value, dropped at scope end —
            // the foundation for `list.filter_map` / `flat_map`. A non-capturing
            // lifted lambda materializes its result (`Some(x)` allocs), and a closure
            // param points to one — so the result is always owned, never borrowed.
            Op::Call { dst: Some(d), result: Some(r), .. }
            | Op::CallFn { dst: Some(d), result: Some(r), .. }
            | Op::CallImport { dst: Some(d), result: Some(r), .. }
            | Op::CallIndirect { dst: Some(d), result: Some(r), .. }
                if r.is_heap() =>
            {
                // A heap loop-carried FEEDER (`new = acc + [x]`): its `i` belongs to
                // the SLOT stream (the slot absorbs `new` via the following SetLocal),
                // folded inside the loop delimiters → `i(id)m`. Otherwise it is a
                // fresh owned object with its own stream (`i`).
                if let Some(&slot) = self.feeder_to_slot.get(d) {
                    // Resolve through `of`: a Dup-initialized slot aliases its source
                    // object (see the sibling arm above).
                    let so = self.s.object_of(slot);
                    self.s.of.insert(*d, so);
                    // STRAIGHT-LINE slot: open its `(id)` CLoop body before the feeder'self.s `i`.
                    if self.line_slots.contains(&slot) {
                        self.s.event(so, '(');
                    }
                    self.s.event(so, 'i');
                } else {
                    self.s.of.insert(*d, *d);
                    self.s.event(*d, 'i');
                }
            }
            // Close a STRAIGHT-LINE slot'self.s `(id)` CLoop body: the feeder'self.s `i` + the drop-old'self.s `d`
            // were already emitted; `)` here makes the per-reassign stream read `(id)` (rc-preserving).
            // A loop slot'self.s SetLocal carries no cert event (its parens are the LoopStart/LoopEnd
            // delimiters); a scalar SetLocal is cert-neutral. So this fires ONLY for a line slot.
            Op::SetLocal { local, .. } if self.line_slots.contains(local) => {
                let so = self.s.object_of(*local);
                self.s.event(so, ')');
            }
            // Open the branch region (format v4, brick 5a): arm events buffer per
            // arm so the flush can group non-self-balancing arms as `{then|else}`.
            // The released merge dst'self.s `i` (the arm'self.s moved-in reference, +1) is a
            // PRE-REGION event — the merge object is acquired at the merge point,
            // outside either arm — so it is emitted before the region opens.
            Op::IfThen { dst, .. } => {
                if let Some(d) = dst {
                    if let Some(&slot) = self.feeder_to_slot.get(d) {
                        // A merge dst that FEEDS a heap slot (`acc = if c then acc + [x]
                        // else acc`): the arms move their value into the merge (+1
                        // received), and the following SetLocal absorbs it into the
                        // slot — route the merge's `i` into the SLOT stream exactly as
                        // the Alloc/heap-call feeders route theirs, so the per-iteration
                        // body folds rc-preserving (`i(iamd)m`) instead of the flat
                        // `iamdm` false imbalance the kernel checker rejects.
                        let so = self.s.object_of(slot);
                        self.s.of.insert(*d, so);
                        if self.line_slots.contains(&slot) {
                            self.s.event(so, '(');
                        }
                        self.s.event(so, 'i');
                    } else if self.released_merge_dsts.contains(d) {
                        self.s.of.insert(*d, *d);
                        self.s.event(*d, 'i');
                    }
                }
                self.s.open_branch();
            }
            // An arm value that still HOLDS its reference when it flows into the merge
            // (`Else/EndIf {{ val }}` with no prior `Consume` — the declared-Result tail-if
            // style, effect_tco::checked) MOVES it there: emit the `m` the explicit-Consume
            // style already has. A val already consumed (balance 0) or never tracked
            // (a scalar) is untouched. The `m` lands in the CLOSING arm'self.s buffer (then
            // at `Else`, else at `EndIf`); then the region switches arm / flushes.
            Op::Else { val } | Op::EndIf { val } => {
                if let Some(v) = val {
                    let val_moves = self.s.of.contains_key(v)
                        && self.s.balance(self.s.object_of(*v)) > 0
                        // An EXPLICITLY-Consumed arm value already emitted its move `m` — the
                        // val-move here would double-count it (the `else base` Var-arm `iammd`
                        // REJECT: the Dup'd value'self.s Consume + this rule both fired on the shared
                        // base object). Only the never-Consumed style (effect-TCO tail-if) reaches here.
                        && !self.consumed_values.contains(v)
                        // Loop-carried machinery keeps its own `(id)` accounting — a slot or
                        // feeder flowing through a branch inside the loop is NOT a move-out
                        // (heap_result_if_append's accumulator would double-`m`).
                        && !self.slots.contains(&self.s.object_of(*v))
                        && !self.feeder_to_slot.contains_key(v)
                        && !self.line_slots.contains(&self.s.object_of(*v));
                    if val_moves {
                        let o = self.s.object_of(*v);
                        self.s.event(o, 'm');
                    }
                }
                if matches!(op, Op::Else { .. }) {
                    self.s.else_branch();
                } else {
                    self.s.flush_branch();
                }
            }
            // A LIVE USE — a read-only borrow or an in-place unique use (`xs[i] = v`
            // via MakeUnique) — on an object whose stream HOLDS ownership (it has a
            // +1 event) is witnessed as `b` (+0, liveness-guarded, brick 5b): a use
            // after the last release makes the proven checker FAULT — owned-object
            // use-after-free is now witnessable, not invisible. An object with no
            // +1 on its stream (a borrowed param used directly) stays event-free:
            // its liveness is the CALLER'self.s obligation, discharged by the call-mode
            // agreement (CallModes.v), not by this stream'self.s count.
            Op::Borrow { v } | Op::MakeUnique { v } => {
                if self.s.of.contains_key(v) {
                    let o = self.s.object_of(*v);
                    let owned = self.s.stream.get(&o).map_or(false, |l| l.contains(['i', 'a']))
                        || self.s.frames.iter().any(|fr| {
                            fr.then_ev.get(&o).map_or(false, |l| l.contains(['i', 'a']))
                                || fr.else_ev.get(&o).map_or(false, |l| l.contains(['i', 'a']))
                        });
                    if owned {
                        self.s.event(o, 'b');
                    }
                }
            }
            // Loop delimiters for a heap loop-carried slot: open `(` on each slot
            // stream when entering a top-level loop, close `)` on leaving — so the
            // slot'self.s per-iteration acquire-new + drop-old reads `(id)`, certifying a
            // rc-preserving body (option C, proved in check_line_unroll_sound).
            Op::LoopStart => {
                if self.depth == 0 {
                    for slot in &self.slots {
                        let so = self.s.object_of(*slot);
                        self.s.event(so, '(');
                    }
                }
                self.depth += 1;
            }
            Op::LoopEnd => {
                self.depth = self.depth.saturating_sub(1);
                if self.depth == 0 {
                    for slot in &self.slots {
                        let so = self.s.object_of(*slot);
                        self.s.event(so, ')');
                    }
                }
            }
            // VALUE-RC (柱C extension) — MIRROR verify_ownership'self.s carrier model so the cert and the
            // executable verifier AGREE on the prim.handle-fed rc case. prim.handle(v) registers the
            // handle as a CARRIER of v'self.s object (no event); rc_inc/rc_dec on a carrier emit `a`/`d`
            // (the proven checker, already rc-aware, verifies the balance). A load64-fed handle has no
            // `of` entry → no event, exactly as before (the differential-test floor).
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.of.insert(*d, o);
                }
            }
            Op::Prim { kind: PrimKind::RcInc, args, .. } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.event(o, 'a');
                }
            }
            Op::Prim { kind: PrimKind::RcDec, args, .. } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.event(o, 'd');
                }
            }
            // `args_get_list` ALLOCATES a fresh owned `List[String]` (argv[1..]) — a +1, like
            // `Alloc`. It feeds no loop, so it gets its own stream (`i`), balanced by the
            // caller'self.s scope-end `DropListStr` (a `d`) or a heap-return move-out (`m`). Without
            // this the heap result would be an unbacked object the cert never opens — the
            // verify_ownership/cert agreement breaks for the env.args body.
            Op::Prim { kind: PrimKind::ArgsGetList, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `env_get` ALLOCATES a fresh owned `Option[String]` (a 0/1-slot block owning
            // the value String when some) — a +1, like `Alloc`. Its name arg is BORROWED
            // (no cert event). Balanced by the caller'self.s scope-end `DropListStr` (`d`) or
            // a heap-return move-out (`m`) — the exact ArgsGetList discipline.
            Op::Prim { kind: PrimKind::EnvGet, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `read_text_file` ALLOCATES a fresh owned `Result[String, String]` (the cap-as-tag
            // block owning one payload String) — a +1, like `Alloc`. Its path arg is BORROWED (the
            // caller still owns it — no cert event). It feeds no loop, so it gets its own stream
            // (`i`), balanced by the caller'self.s scope-end `DropListStr` (a `d`) or a heap-return
            // move-out (`m`). Without this the heap result would be an unbacked object the cert
            // never opens — the verify_ownership/cert agreement breaks for the fs.read_text body.
            Op::Prim { kind: PrimKind::ReadTextFile, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `read_dir` ALLOCATES a fresh owned `Result[List[String], String]` (the cap-as-tag
            // block owning one payload `List[String]`) — a +1, like `ReadTextFile`/`Alloc`. Its
            // path arg is BORROWED (no cert event). Its own stream (`i`), balanced by the
            // caller'self.s scope-end recursive `DropResultListStr` (`d`) or a heap-return move-out
            // (`m`). Without this the heap result would be an unbacked object the cert never
            // opens — the verify_ownership/cert agreement breaks for the fs.list_dir body.
            Op::Prim { kind: PrimKind::ReadDir, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `write_text_file` ALLOCATES a fresh owned `Result[Unit, String]` (the cap-as-tag
            // block — Ok carries NO payload, Err owns one message String) — a +1, like
            // `ReadTextFile`/`Alloc`. Both its args (path + content) are BORROWED (no cert event).
            // Its own stream (`i`), balanced by the caller'self.s scope-end flat `DropListStr` (`d`) or a
            // heap-return move-out (`m`). Without this the heap result would be an unbacked object
            // the cert never opens — the verify_ownership/cert agreement breaks for the fs.write body.
            Op::Prim { kind: PrimKind::WriteTextFile, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `make_dir` ALLOCATES a fresh owned `Result[Unit, String]` (the cap-as-tag block —
            // Ok carries NO payload, Err owns one message String) — a +1, EXACTLY like
            // `WriteTextFile`/`Alloc`. Its path arg is BORROWED (no cert event). Its own stream
            // (`i`), balanced by the caller'self.s scope-end flat `DropListStr` (`d`) or a heap-return
            // move-out (`m`). Without this the heap result would be an unbacked object the cert
            // never opens — the verify_ownership/cert agreement breaks for the fs.mkdir_p body.
            Op::Prim { kind: PrimKind::MakeDir, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `remove_all` ALLOCATES a fresh owned `Result[Unit, String]` (the cap-as-tag block —
            // Ok carries NO payload, Err owns one message String) — a +1, EXACTLY like
            // `MakeDir`/`WriteTextFile`/`Alloc`. Its path arg is BORROWED (no cert event). Its own
            // stream (`i`), balanced by the caller'self.s scope-end flat `DropListStr` (`d`) or a
            // heap-return move-out (`m`). Without this the heap result would be an unbacked object
            // the cert never opens — the verify_ownership/cert agreement breaks for the
            // fs.remove_all body.
            Op::Prim { kind: PrimKind::RemoveAll, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // `read_line` ALLOCATES a fresh owned canonical `String` (one line of stdin) — a +1,
            // like `Alloc`. No args. It feeds no loop, so it gets its own stream (`i`), balanced by
            // the caller'self.s scope-end flat `Drop` (a String owns no nested handles) or a heap-return
            // move-out (`m`). Without this the heap result would be an unbacked object the cert
            // never opens — the verify_ownership/cert agreement breaks for the io.read_line body.
            Op::Prim { kind: PrimKind::ReadLine | PrimKind::ReadNBytes, dst: Some(d), .. } => {
                self.s.of.insert(*d, *d);
                self.s.event(*d, 'i');
            }
            // No refcount change: Const/Pure/IntBinOp/scalar SetLocal, and a call
            // with a void/scalar result (its heap-handle args are borrowed).
            _ => {}
        }
    }
}

/// The number of `i` events [`ownership_certificate`] credits to branch-MERGE
/// dsts: the RELEASED merges (the arm's moved-in reference, later Consumed/
/// Dropped/val-flowed/returned) plus the slot-FEEDER merges (`acc = if c then
/// acc + [x] else acc`, whose `i` routes into the loop-carried slot stream).
/// Both are backed by the arm value's real producer — the merge is a reference
/// changing hands (the wasm merge local.set), not a synthetic `+1`. classify's
/// borrow-by-default backing gate uses THIS count so the gate and the emission
/// stay in lockstep by construction (one credit per IfThen op occurrence,
/// mirroring `CertScan::step`'s pre-region emission exactly).
pub fn merge_dst_i_credits(func: &MirFunction) -> usize {
    let (feeder_to_slot, _, _) = loop_carried_slots(func);
    let mut merge_dsts: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    let mut released: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    for op in &func.ops {
        match op {
            Op::IfThen { dst: Some(d), .. } => {
                merge_dsts.insert(*d);
            }
            Op::Consume { v } | Op::Drop { v } | Op::DropListStr { v } => {
                if merge_dsts.contains(v) {
                    released.insert(*v);
                }
            }
            Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } => {
                if merge_dsts.contains(v) {
                    released.insert(*v);
                }
            }
            _ => {}
        }
    }
    if let Some(r) = func.ret {
        if merge_dsts.contains(&r) {
            released.insert(r);
        }
    }
    func.ops
        .iter()
        .filter(|op| match op {
            Op::IfThen { dst: Some(d), .. } => {
                feeder_to_slot.contains_key(d) || released.contains(d)
            }
            _ => false,
        })
        .count()
}

pub fn ownership_certificate(func: &MirFunction) -> String {
    let (feeder_to_slot, slots, line_slots) = loop_carried_slots(func);
    let mut depth: u32 = 0;
    let mut s = Streams::new();

    // A branch-MERGE dst (`Op::IfThen {{ dst }}`) that is later RELEASED — consumed
    // by an OUTER frame (the nested monadic-`!` chain: the inner match's merged
    // Result moves into the outer merge) or returned — RECEIVES the arm value each
    // arm moved in (the arm's `m`). Record that move-in as the merge object's `i`
    // so its later `m`/`d` balances ("im", the physical rc: the arm's −1 and the
    // merge's +1 are the same reference changing hands). An UNUSED merge dst stays
    // event-free exactly as before. Without this the chained-`!` witness read as a
    // bare `m` and the proven checker REJECTED it (flight-evidence-gaps F8).
    let mut released_merge_dsts: std::collections::HashSet<crate::ValueId> =
        std::collections::HashSet::new();
    {
        let mut merge_dsts: std::collections::HashSet<crate::ValueId> =
            std::collections::HashSet::new();
        for op in &func.ops {
            match op {
                Op::IfThen { dst: Some(d), .. } => {
                    merge_dsts.insert(*d);
                }
                Op::Consume { v } | Op::Drop { v } | Op::DropListStr { v } => {
                    if merge_dsts.contains(v) {
                        released_merge_dsts.insert(*v);
                    }
                }
                // An INNER merge flowing out as an OUTER arm value (`Else/EndIf {{ val }}`
                // — the effect-TCO nested-if chain) is released the same way: the val-move
                // rule below emits its `m`.
                Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } => {
                    if merge_dsts.contains(v) {
                        released_merge_dsts.insert(*v);
                    }
                }
                _ => {}
            }
        }
        if let Some(r) = func.ret {
            if merge_dsts.contains(&r) {
                released_merge_dsts.insert(r);
            }
        }
    }

    // The set of values EXPLICITLY moved out by an `Op::Consume` — the arm-value move
    // for the LitStr/Var/concat arms (`lower_heap_result_arm`). Such a value's `m` is
    // ALREADY on its object's stream, so the `Else/EndIf {val}` val-move rule below must
    // NOT emit a SECOND `m` for it. The per-object `balance > 0` guard alone cannot catch
    // this when the value ALIASES a still-live scope local (`else base` — the Var arm
    // Dups base, so the shared object keeps balance 1 after the Consume, and the val-move
    // double-`m`'d it → the `iammd` REJECT). Only the val-move-ONLY style (the effect-TCO
    // declared-Result tail-if, whose arms never Consume) should reach the rule.
    let consumed_values: std::collections::HashSet<crate::ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Consume { v } => Some(*v),
            _ => None,
        })
        .collect();

    // Heap params are BORROWED (the v1 calling convention): the CALLER owns the
    // reference and releases it, so a param contributes NO `i` event — that `+1`
    // would be SYNTHETIC, unbacked by any runtime `Alloc`/`rc_inc` (the gate-blind
    // use-after-free class). We still register the object identity (`of`) so that
    // a body which releases (`Drop`/`Consume`) or returns a borrowed param WITHOUT
    // first acquiring its own reference (a `Dup`) emits a `d`/`m` at rc 0 — which
    // the proven checker FAULTS (REJECT), exactly the double-free that owning the
    // caller's reference would cause. A `Dup` of the param emits the real `a`.
    for p in &func.params {
        if p.repr.is_heap() {
            s.of.insert(p.value, p.value);
        }
    }

    // Decomposed (#781, cog 123): the per-op emission lives in `CertScan::step`;
    // the pre-scan state moved into the scan struct verbatim.
    let mut scan = CertScan {
        depth,
        s,
        released_merge_dsts,
        consumed_values,
        feeder_to_slot,
        slots,
        line_slots,
    };
    for op in &func.ops {
        scan.step(op);
    }

    // Defensive: a dangling IfThen (no EndIf — malformed MIR) still flushes, so
    // its buffered arm events land on the stream (and unbalance ⟹ reject) rather
    // than vanish.
    while !scan.s.frames.is_empty() {
        scan.s.flush_branch();
    }

    // A heap return is MOVED OUT to the caller (a −1) — a move, hence `m`.
    if let Some(r) = func.ret {
        if scan.s.of.contains_key(&r) {
            let o = scan.s.object_of(r);
            scan.s.event(o, 'm');
        }
    }

    let mut out = String::new();
    for o in &scan.s.order {
        out.push_str(&scan.s.stream[o]);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        verify_ownership, CallArg, Capability, Init, MirFunction, MirParam, Op, PrimKind, Repr, RtFn,
        ValueId, PLACEHOLDER_LAYOUT,
    };

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "f".into(), ops, ..Default::default() }
    }

    #[test]
    fn value_rc_carrier_balance_is_certified() {
        // 柱C extension: `prim.handle(o)` makes the handle a CARRIER of o's object, so an UNBALANCED
        // rc_inc on it is now a Leak that BOTH verify_ownership and the cert catch — the Value-rc class
        // that used to be invisible in the prim region. `i`(alloc) + `a`(rc_inc on carrier) + `d`(drop)
        // = rc 1 at end → leak.
        let (o, h) = (ValueId(0), ValueId(1));
        let unbalanced = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&unbalanced), "iad\n");
        assert!(verify_ownership(&unbalanced).is_err());

        // A BALANCED rc_inc/rc_dec on the carrier → both ACCEPT (rc 0 at end).
        let balanced = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Prim { kind: PrimKind::RcDec, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&balanced), "iadd\n");
        assert_eq!(verify_ownership(&balanced), Ok(()));

        // A load64-fed rc (NO prim.handle carrier) stays UNMODELED — the differential-test floor: the
        // RcInc on a non-carrier handle emits no `a` and the verifier no-ops it, so the function is
        // just the balanced Alloc+Drop ("id").
        let load_fed = func(vec![
            Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(h), args: vec![o] },
            Op::Prim { kind: PrimKind::RcInc, dst: None, args: vec![h] },
            Op::Drop { v: o },
        ]);
        assert_eq!(ownership_certificate(&load_fed), "id\n");
        assert_eq!(verify_ownership(&load_fed), Ok(()));
    }

    #[test]
    fn alias_then_drops_is_one_balanced_object() {
        // a = Alloc; b = Dup a; Drop a; Drop b  → ONE object (a), stream "iidd".
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: b, src: a },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        // Alloc(i), Dup→alias(a), Drop(d), Drop(d): the alias acquire is `a`.
        assert_eq!(ownership_certificate(&f), "iadd\n");
        assert_eq!(verify_ownership(&f), Ok(())); // checker would accept ⟺ this
    }

    #[test]
    fn two_objects_each_balanced() {
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Alloc { dst: b, repr: heap(), init: Init::Opaque },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        // object a: "id", object b: "id" — two balanced lines.
        assert_eq!(ownership_certificate(&f), "id\nid\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn loop_carried_accumulator_folds_to_one_slot_stream() {
        // The heap-loop-carried accumulator (option C): `acc` is alloc'd, then each
        // iteration allocs a NEW object (the `acc + [x]` feeder), drops the OLD acc,
        // and rebinds `acc = new` via SetLocal; finally `acc` is returned (moved out).
        //   acc=Alloc; loop { new=Alloc; Drop acc; SetLocal acc,new }; ret acc
        // The slot folds to ONE stream `i(id)m` — acquire once; loop body acquire-new +
        // drop-old (a rc-preserving body); move out — accepted by the proven check_cert_lc.
        let (acc, new) = (ValueId(0), ValueId(1));
        let mut f = func(vec![
            Op::Alloc { dst: acc, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: new, repr: heap(), init: Init::Opaque },
            Op::Drop { v: acc },
            Op::SetLocal { local: acc, src: new },
            Op::LoopEnd,
        ]);
        f.ret = Some(acc);
        assert_eq!(ownership_certificate(&f), "i(id)m\n");
        // The Rust-side checker accepts it too (SetLocal rebind preserves the slot
        // invariant) — its verdict matches the proven check_cert_lc on the cert.
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn loop_carried_leaky_body_is_rejected() {
        // A loop body that allocs but never drops the old acc → the slot stream is
        // `i(i)m` (loop body NOT rc-preserving: net +1) → REJECT, both here and in Coq.
        let (acc, new) = (ValueId(0), ValueId(1));
        let mut f = func(vec![
            Op::Alloc { dst: acc, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: new, repr: heap(), init: Init::Opaque },
            Op::SetLocal { local: acc, src: new },
            Op::LoopEnd,
        ]);
        f.ret = Some(acc);
        assert_eq!(ownership_certificate(&f), "i(i)m\n");
        // verify_ownership flags the leaked old `acc` object (the dropped Alloc never
        // released before rebind) — the cert faithfully carries the rejection.
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn swap_carried_buffer_folds_dup_feeder_into_the_slot_stream() {
        // The SWAP-CARRY shape (`cur = merged`, loop_buffer_churn / C-131): since the
        // whole-var alias-edge elision the rebind lowers as `Dup tmp = merged;
        // Drop cur; SetLocal cur = tmp` — the slot's feeder is a DUP dst, not an
        // Alloc/call result. The Dup's `a` must route into the slot stream so the
        // per-iteration acquire-new + drop-old reads `(ad)` (rc-preserving); flat, the
        // in-loop drop-old + scope-end drop read `idd` — a FALSE double-free (the
        // corpus-wall REJECT the first develop Trust Spine run caught).
        let (cur, merged, tmp) = (ValueId(0), ValueId(1), ValueId(2));
        let f = func(vec![
            Op::Alloc { dst: cur, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: merged, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: tmp, src: merged },
            Op::Drop { v: cur },
            Op::SetLocal { local: cur, src: tmp },
            Op::Drop { v: merged },
            Op::LoopEnd,
            Op::Drop { v: cur },
        ]);
        assert_eq!(ownership_certificate(&f), "i(ad)d\nid\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn swap_carry_without_drop_old_is_rejected() {
        // The same swap-carry but the OLD buffer is never dropped in the body — a
        // real leak: the slot stream reads `i(a)d` (body nets +1, not rc-preserving)
        // → REJECT, and verify_ownership flags the leaked original object.
        let (cur, merged, tmp) = (ValueId(0), ValueId(1), ValueId(2));
        let f = func(vec![
            Op::Alloc { dst: cur, repr: heap(), init: Init::Opaque },
            Op::LoopStart,
            Op::Alloc { dst: merged, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: tmp, src: merged },
            Op::SetLocal { local: cur, src: tmp },
            Op::Drop { v: merged },
            Op::LoopEnd,
            Op::Drop { v: cur },
        ]);
        assert_eq!(ownership_certificate(&f), "i(a)d\nid\n");
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn leak_shows_as_unbalanced_object() {
        // a allocated, never dropped → stream "i" (rc ends 1 = leak).
        let a = ValueId(0);
        let f = func(vec![Op::Alloc { dst: a, repr: heap(), init: Init::Opaque }]);
        assert_eq!(ownership_certificate(&f), "i\n");
        // verify_ownership flags it too — the certificate faithfully carries it.
        assert!(verify_ownership(&f).is_err());
    }

    // ── faithfulness mechanism ──
    // The certificate must honestly represent the ownership pass: the proven
    // checker's verdict on `ownership_certificate(f)` must equal `verify_ownership(f)`'s.
    // Otherwise the PCC chain certifies the wrong thing. We pin it over many
    // random WELL-FORMED ownership sequences.

    /// Re-run the proven checker's decision in Rust (mirrors the Coq `check_bc`):
    /// every line's stream must never dec-below-zero and must end at 0, with the
    /// format-v4 branch rule — `{then|else}` arms both execute from the current
    /// count, must not fault, and must AGREE on the leaving count.
    fn cert_all_balanced(cert: &str) -> bool {
        // The flat fold (format v1 alphabet + the 5b `b` guard); None = fault.
        fn fold(seg: &str, mut rc: i64) -> Option<i64> {
            for c in seg.chars() {
                match c {
                    // i/a = +1 (fresh/alias), d/m = −1 (release/move-out).
                    'i' | 'a' => rc += 1,
                    'd' | 'm' => {
                        if rc == 0 {
                            return None; // double-free / use-after-move
                        }
                        rc -= 1;
                    }
                    // b = +0 live use — faults on a dead object (use-after-free),
                    // exactly the Coq `Borrow` guard.
                    'b' => {
                        if rc == 0 {
                            return None;
                        }
                    }
                    _ => {}
                }
            }
            Some(rc)
        }
        cert.lines().all(|line| {
            let mut rc: i64 = 0;
            let mut rest = line;
            while let Some(open) = rest.find('{') {
                rc = match fold(&rest[..open], rc) {
                    Some(r) => r,
                    None => return false,
                };
                let close = match rest[open..].find('}') {
                    Some(c) => open + c,
                    None => return false, // unterminated branch — malformed
                };
                let (t, e) = match rest[open + 1..close].split_once('|') {
                    Some(p) => p,
                    None => return false,
                };
                match (fold(t, rc), fold(e, rc)) {
                    (Some(rt), Some(re)) if rt == re => rc = rt, // arms AGREE
                    _ => return false, // an arm faults or the arms disagree
                }
                rest = &rest[close + 1..];
            }
            match fold(rest, rc) {
                Some(r) => r == 0, // leak iff != 0
                None => false,
            }
        })
    }

    /// A tiny seeded PRNG (no dep), so the random test is deterministic.
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    }

    /// Build a random ownership op sequence over LIVE handles, now including
    /// BRANCH regions (format v4): agreeing arms (both alias the same object —
    /// grouped `{a|a}`, net +1), per-arm self-balancing arms (flat flush), and
    /// occasionally DISAGREEING arms (`{a|}` — both the grouped cert and
    /// verify_ownership's branch join must reject). Leftover-undropped handles
    /// make it a leak — so the corpus spans accept and reject across the flat,
    /// borrow and branch machinery, and the test pins that the cert verdict
    /// EQUALS verify_ownership's on every seed.
    fn gen_wellformed(seed: u64) -> MirFunction {
        let mut st = seed.wrapping_add(1);
        let mut live: Vec<ValueId> = Vec::new();
        let mut next: u32 = 0;
        let mut ops: Vec<Op> = Vec::new();
        let steps = 3 + (next_rand(&mut st) % 9) as usize;
        for _ in 0..steps {
            let choice = next_rand(&mut st) % 6;
            match choice {
                0 => {
                    // Alloc a fresh object.
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Alloc { dst: v, repr: heap(), init: Init::Opaque });
                    live.push(v);
                }
                1 if !live.is_empty() => {
                    // Dup a live handle → a new handle on the same object.
                    let src = live[(next_rand(&mut st) as usize) % live.len()];
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Dup { dst: v, src });
                    live.push(v);
                }
                2 if !live.is_empty() => {
                    // Drop a live handle.
                    let i = (next_rand(&mut st) as usize) % live.len();
                    let v = live.remove(i);
                    ops.push(Op::Drop { v });
                }
                3 if !live.is_empty() => {
                    // Borrow (a `b` event on the owned stream — liveness-guarded).
                    let v = live[(next_rand(&mut st) as usize) % live.len()];
                    ops.push(Op::Borrow { v });
                }
                4 if !live.is_empty() => {
                    // An AGREEING branch: each arm acquires one alias of the same
                    // live object (net +1 both ways — the heap-result-branch
                    // class, grouped `{a|a}`). The runtime holds ONE new alias
                    // whichever arm ran; hand `y` to the pool (`z` is the other
                    // path's handle — same object, never used again).
                    let x = live[(next_rand(&mut st) as usize) % live.len()];
                    let (c, y, z) = (ValueId(next), ValueId(next + 1), ValueId(next + 2));
                    next += 3;
                    ops.push(Op::Const { dst: c });
                    ops.push(Op::IfThen { cond: c, dst: None });
                    ops.push(Op::Dup { dst: y, src: x });
                    ops.push(Op::Else { val: None });
                    ops.push(Op::Dup { dst: z, src: x });
                    ops.push(Op::EndIf { val: None });
                    live.push(y);
                }
                5 if !live.is_empty() && next_rand(&mut st) % 3 == 0 => {
                    // A DISAGREEING branch (one arm aliases, the other does not —
                    // a path-dependent count): the grouped cert `{a|}` and the
                    // branch join must BOTH reject, and both are sticky, so the
                    // verdicts stay equal however generation continues.
                    let x = live[(next_rand(&mut st) as usize) % live.len()];
                    let (c, y) = (ValueId(next), ValueId(next + 1));
                    next += 2;
                    ops.push(Op::Const { dst: c });
                    ops.push(Op::IfThen { cond: c, dst: None });
                    ops.push(Op::Dup { dst: y, src: x });
                    ops.push(Op::Else { val: None });
                    ops.push(Op::EndIf { val: None });
                }
                _ => {}
            }
        }
        func(ops)
    }

    #[test]
    fn certificate_verdict_matches_verify_ownership() {
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            let cert_ok = cert_all_balanced(&ownership_certificate(&f));
            let verify_ok = verify_ownership(&f).is_ok();
            assert_eq!(
                cert_ok, verify_ok,
                "seed {seed}: certificate says {cert_ok}, verify_ownership says {verify_ok}\nops: {:?}",
                f.ops
            );
        }
    }

    include!("certificate_p2.rs");
}
