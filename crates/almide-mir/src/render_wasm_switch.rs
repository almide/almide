// ── Dense integer dispatch: `br_table` for the match if-else chain ─────────
//
// #882. `match` lowers to a FLAT marker chain — one
// `ConstInt k; IntBinOp Eq subj k; IfThen` triple per arm, each successive test
// living inside the previous arm's `else`. The wasm renderer reconstructs that
// literally as nested `if`/`else`, so a 12-arm match over `Int` literals or a
// flat variant's tags cost up to 12 compare-and-branch pairs per dispatch —
// O(arms), and wasmtime does not rebuild a jump table out of it.
//
// This module recognizes that chain and, when the keys are DENSE (contiguous or
// near-contiguous), re-renders it as a single `br_table` — O(1). It is a
// RENDER-level rewrite only: the MIR and its ownership certificate are untouched
// (exactly like the `Fuser` and the BCE versioner), and the arm bodies are
// re-rendered from the SAME op ranges through the SAME `render_op_range`, so
// every arm's ops, drops and nested control flow are emitted byte-for-byte as
// before. Only the dispatch that SELECTS an arm changes.
//
// Why this is observationally identical to the chain:
//   - Each test `subj == k_t` is evaluated INSIDE arm t-1's `else`, i.e. only
//     after arms 0..t-1 were NOT taken — so no arm body can have run before any
//     test. Hoisting the subject read to the top of the dispatch therefore
//     cannot observe a different value.
//   - The recognizer requires the `else` arm to consist of EXACTLY the next
//     triple (nothing before it, nothing after the nested `EndIf`), so there is
//     no intervening effect to reorder.
//   - Exactly one arm runs in both forms, and it is the arm with the matching
//     key (keys are required distinct), or the default when none matches.
//
// The one place the two forms could disagree is the index computation: a
// `br_table` index is i32, while the subject is i64. A bare `i32.wrap_i64` would
// alias `2^32 + k` onto arm `k`. So the emitted form first discharges the FULL
// i64 range with one unsigned `(subj - min) >= span` test that branches to the
// default — negative, above-span and beyond-2^32 subjects all take it.

/// A recognized dense dispatch: the whole `ConstInt/Eq/IfThen … EndIf` chain
/// `func.ops[start..=end_idx]`, decomposed into the arms a `br_table` needs.
pub(crate) struct SwitchPlan {
    /// The value every arm's test compares against — read ONCE, at the top.
    subj: ValueId,
    /// The chain's result local (the OUTERMOST `IfThen`'s `dst`); `None` for a
    /// statement-position chain. The inner levels' `dst`s are collapsed away.
    dst: Option<ValueId>,
    /// Per arm, ascending by key: `(key, then-arm ops [lo, hi), arm result)`.
    arms: Vec<SwitchArm>,
    /// The innermost `else` — the wildcard. Also the `br_table` default target.
    default_ops: (usize, usize),
    default_val: Option<ValueId>,
    /// Index of the OUTERMOST `EndIf`: the last op the plan consumes.
    end_idx: usize,
}

/// One arm of a [`SwitchPlan`].
struct SwitchArm {
    key: i64,
    ops: (usize, usize),
    val: Option<ValueId>,
}

/// Below this many arms the chain is already cheap (≤ 2 compares on average)
/// and a table plus its two guard blocks would cost more code than it saves.
const SWITCH_MIN_ARMS: usize = 4;
/// Reject a sparse key set: a table has one entry per SLOT, not per arm, so
/// `{1, 5, 900}` would emit 900 entries to dispatch 3 arms. Four slots per arm
/// is the break-even — one table entry is a byte or two, one elided compare
/// sequence is a dozen.
const SWITCH_MAX_SLOTS_PER_ARM: i128 = 4;
/// Absolute cap on the table size, so a pathological-but-dense key set (say
/// `0..=100_000`) cannot blow up the module.
const SWITCH_MAX_SPAN: i128 = 1024;

/// The `Else`/`EndIf` closing `ops[if_idx]`, by depth-matched scan. `None` if the
/// `if` has no `else`, or is not closed before `limit`.
fn if_arm_bounds(ops: &[Op], if_idx: usize, limit: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut else_idx: Option<usize> = None;
    for (i, op) in ops.iter().enumerate().take(limit).skip(if_idx + 1) {
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::Else { .. } if depth == 0 => {
                // Two `Else` markers at one depth would be malformed MIR; bail
                // rather than mis-slice the arm.
                if else_idx.is_some() {
                    return None;
                }
                else_idx = Some(i);
            }
            Op::EndIf { .. } => {
                if depth == 0 {
                    return Some((else_idx?, i));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The `subj == literal` test a chain level opens with: `ops[i]` a `ConstInt`,
/// `ops[i + 1]` an `Eq` against it, `ops[i + 2]` the `IfThen` on the result.
/// Returns `(subject, key, dst)`. Both operand orders are accepted.
///
/// The `occ` guards are what make dropping these three ops sound: the key local
/// and the Bool local must each occur EXACTLY twice program-wide (their def plus
/// this one use), so nothing outside the chain can read the locals the rewrite
/// stops writing.
fn eq_test_at(
    ops: &[Op],
    occ: &BTreeMap<ValueId, usize>,
    i: usize,
) -> Option<(ValueId, i64, Option<ValueId>)> {
    let (Op::ConstInt { dst: kd, value: key }, Op::IntBinOp { dst: c, op: IntOp::Eq, a, b }) =
        (ops.get(i)?, ops.get(i + 1)?)
    else {
        return None;
    };
    let Op::IfThen { cond, dst } = ops.get(i + 2)? else { return None };
    if cond != c || occ.get(kd) != Some(&2) || occ.get(c) != Some(&2) {
        return None;
    }
    let subj = match (*a == *kd, *b == *kd) {
        (true, false) => *b,
        (false, true) => *a,
        // `k == k` (or neither operand is the key) is not a dispatch on a subject.
        _ => return None,
    };
    Some((subj, *key, *dst))
}

/// Recognize a dense `match` chain starting at `ops[start]`, within `[start, limit)`.
/// `None` — and the caller renders the ordinary nested `if` — whenever anything
/// about the shape, the key density or the local-occurrence guards fails.
pub(crate) fn plan_switch(
    ops: &[Op],
    occ: &BTreeMap<ValueId, usize>,
    start: usize,
    limit: usize,
) -> Option<SwitchPlan> {
    if std::env::var("ALMIDE_NO_BR_TABLE").is_ok() {
        return None;
    }
    let (subj, _, _) = eq_test_at(ops, occ, start)?;
    let mut arms: Vec<SwitchArm> = Vec::new();
    let mut dsts: Vec<Option<ValueId>> = Vec::new();
    let mut endifs: Vec<usize> = Vec::new();
    let mut i = start;
    // Peel levels while the else arm opens with another test on the SAME subject.
    // A different subject (a genuinely nested match) ends this chain and becomes
    // the default arm — where `render_op_range` may recognize it in its own right.
    while let Some((s, key, dst)) = eq_test_at(ops, occ, i) {
        // An inner level's `dst` is consumed ONLY by the enclosing `EndIf`; the
        // rewrite collapses every level onto the outermost `dst`, so any other
        // reader would go unwritten. Mixing value- and statement-position levels
        // (`dst` Some vs None) cannot be one expression either.
        let inner_dst_ok = match (arms.is_empty(), dst) {
            (true, _) => true,
            (false, Some(d)) => occ.get(&d) == Some(&2),
            (false, None) => true,
        };
        if s != subj || !inner_dst_ok || dst.is_some() != dsts.first().unwrap_or(&dst).is_some() {
            break;
        }
        let (else_idx, endif_idx) = if_arm_bounds(ops, i + 2, limit)?;
        arms.push(SwitchArm { key, ops: (i + 3, else_idx), val: arm_result(&ops[else_idx]) });
        dsts.push(dst);
        endifs.push(endif_idx);
        i = else_idx + 1;
    }
    let n = arms.len();
    if n < SWITCH_MIN_ARMS {
        return None;
    }
    // The else arm of level t must be EXACTLY level t+1's `if` — nothing may
    // follow the nested `EndIf` — and it must yield level t+1's `dst` unchanged.
    // That is what lets the rewrite forget the intermediate levels entirely.
    for t in 0..n - 1 {
        if endifs[t] != endifs[t + 1] + 1 || arm_result(&ops[endifs[t]]) != dsts[t + 1] {
            return None;
        }
    }
    let plan = SwitchPlan {
        subj,
        dst: dsts[0],
        arms,
        default_ops: (i, endifs[n - 1]),
        default_val: arm_result(&ops[endifs[n - 1]]),
        end_idx: endifs[0],
    };
    plan.dense().then_some(plan)
}

/// The value an `Else`/`EndIf` marker leaves on the stack for its arm.
fn arm_result(op: &Op) -> Option<ValueId> {
    match op {
        Op::Else { val } | Op::EndIf { val } => *val,
        _ => None,
    }
}

impl SwitchPlan {
    /// Keys distinct, and the table small enough to be worth emitting.
    fn dense(&self) -> bool {
        let mut keys: Vec<i64> = self.arms.iter().map(|a| a.key).collect();
        keys.sort_unstable();
        if keys.windows(2).any(|w| w[0] == w[1]) {
            return false;
        }
        let span = i128::from(*keys.last().expect("min-arms checked")) - i128::from(keys[0]) + 1;
        span <= SWITCH_MAX_SPAN && span <= SWITCH_MAX_SLOTS_PER_ARM * self.arms.len() as i128
    }

    /// `(lowest key, table slot count)`.
    fn table_range(&self) -> (i64, i64) {
        let lo = self.arms.iter().map(|a| a.key).min().expect("min-arms checked");
        let hi = self.arms.iter().map(|a| a.key).max().expect("min-arms checked");
        (lo, hi - lo + 1)
    }
}

/// Render one recognized chain as a `br_table` dispatch.
///
/// Shape (flat wat; the arm bodies come from `render_op_range` on the SAME op
/// ranges the nested `if` would have used):
///
/// ```text
///   block $done (result T)      ;; absent when the chain is in statement position
///   block $def
///   block $a{n-1} … block $a0
///     (i64.ge_u (subj - min) span)   br_if $def   ;; full-i64 range guard
///     (i32.wrap_i64 (subj - min))    br_table $a0 … $a{n-1} $def
///   end                              ;; falls through to arm 0 (lowest key)
///   <arm 0> br $done
///   …
///   end                              ;; $def
///   <wildcard>
///   end                              ;; $done
///   (local.set $dst)
/// ```
fn render_switch(
    ctx: &RenderFnCtx,
    st: &mut RenderFnState,
    plan: &SwitchPlan,
    region: Option<(usize, &BTreeSet<usize>)>,
    body: &mut String,
) {
    // A block boundary: every deferred expression must be materialized first.
    st.fuser.flush_all(body);
    let id = st.switch_ctr;
    st.switch_ctr += 1;
    if std::env::var("ALMIDE_DBG_SWITCH").is_ok() {
        eprintln!("[switch] {} arms={}", ctx.func.name, plan.arms.len());
    }
    let res = match plan.dst {
        Some(d) => format!(
            " (result {})",
            wasm_ty(ctx.reprs.get(&d).copied().unwrap_or(SCALAR_REPR))
        ),
        None => String::new(),
    };
    let (min, span) = plan.table_range();
    // `subj - min` twice rather than through a scratch local: the renderer has no
    // spare ValueId to declare one, and two i64 subtracts are free next to the
    // branch. `min == 0` (the flat-variant tag case) needs neither.
    let biased = if min == 0 {
        format!("(local.get {})", local(plan.subj))
    } else {
        format!("(i64.sub (local.get {}) (i64.const {min}))", local(plan.subj))
    };
    let mut by_key: Vec<&SwitchArm> = plan.arms.iter().collect();
    by_key.sort_by_key(|a| a.key);

    body.push_str(&format!("    block $sw{id}_done{res}\n    block $sw{id}_def\n"));
    for j in (0..by_key.len()).rev() {
        body.push_str(&format!("    block $sw{id}_a{j}\n"));
    }
    // The i32 `br_table` index cannot alias a far-away i64 subject onto an arm:
    // one UNSIGNED compare rejects everything outside [min, min + span) first —
    // a subject below `min` wraps to a huge unsigned value and is caught too.
    body.push_str(&format!(
        "    (i64.ge_u {biased} (i64.const {span}))\n    br_if $sw{id}_def\n\
         \x20   (i32.wrap_i64 {biased})\n    br_table"
    ));
    for slot in 0..span {
        match by_key.iter().position(|a| a.key == min + slot) {
            // A HOLE inside the span (the density budget allows a few) is just
            // another way to reach the wildcard.
            None => body.push_str(&format!(" $sw{id}_def")),
            Some(j) => body.push_str(&format!(" $sw{id}_a{j}")),
        }
    }
    body.push_str(&format!(" $sw{id}_def\n"));
    for arm in &by_key {
        body.push_str("    end\n");
        render_op_range(ctx, st, arm.ops.0, arm.ops.1, region, body);
        st.fuser.flush_all(body);
        body.push_str(&switch_arm_val(arm.val));
        body.push_str(&format!("    br $sw{id}_done\n"));
    }
    // The arm loop's last `end` closed the last `$a…`; this one closes `$def`,
    // so the wildcard falls through to `$done` carrying its own value.
    body.push_str("    end\n");
    render_op_range(ctx, st, plan.default_ops.0, plan.default_ops.1, region, body);
    st.fuser.flush_all(body);
    body.push_str(&switch_arm_val(plan.default_val));
    body.push_str("    end\n");
    if let Some(d) = plan.dst {
        body.push_str(&format!("    (local.set {})\n", local(d)));
    }
}

/// An arm's result, pushed for the enclosing block — the same `(local.get …)`
/// the nested-`if` render leaves on the stack at an `Else`/`EndIf`.
fn switch_arm_val(v: Option<ValueId>) -> String {
    v.map(|v| format!("      (local.get {})\n", local(v))).unwrap_or_default()
}
