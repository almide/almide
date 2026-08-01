//! Executable model of the logical-time race semantics
//! (docs/roadmap/active/logical-time-async.md + fan-v2.md).
//!
//! Three artifacts are compared over an exhaustive small scope:
//!
//!   REF   the reference semantics: merge all branch events by (time, branch),
//!         the first decisive event (Complete | Trap) determines the outcome;
//!         consumed fuel = charges strictly preceding the decisive event.
//!   SEQ   the sequential list-order scan with shrinking caps and deferred
//!         traps (the wasm lowering strategy), including the consumed-fuel
//!         reconstruction from capped runs only.
//!   ADV   an adversarial parallel implementation: EVERY physical schedule of
//!         branch steps, with pruning exactly at the cap rule, explored by
//!         memoized DFS. Confluence check: the set of reachable outcomes must
//!         be a singleton equal to REF.
//!
//! Plus the nesting check: the merge-order charge stream that REF says occurs
//! must be exactly reconstructible from what the capped runs revealed, so an
//! enclosing bounded region observes identical streaming consumption.
//!
//! A mismatch anywhere is a design bug, not an implementation bug — that is
//! the point of the model.

use std::collections::{BTreeSet, HashSet};

// ---------------------------------------------------------------- traces

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Terminal {
    Complete, // returns a value (identified by branch index)
    Trap,
    Diverge, // keeps charging cost-1 forever
}

#[derive(Clone, Debug)]
struct Trace {
    charges: Vec<u64>,
    terminal: Terminal,
}

/// How a branch run under a budget ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    Complete(u64), // at cumulative time t
    Trap(u64),
    Exhaust(u64), // consumed t, could not afford the next charge (or hit budget while diverging)
}

impl End {
    fn time(self) -> u64 {
        match self {
            End::Complete(t) | End::Trap(t) | End::Exhaust(t) => t,
        }
    }
}

/// Run one branch alone under `budget` (check-then-charge; Diverge = endless
/// cost-1 charges). Deterministic: a pure function of (trace, budget).
fn run_branch(tr: &Trace, budget: u64) -> End {
    let mut cum = 0u64;
    for &c in &tr.charges {
        if cum + c > budget {
            return End::Exhaust(cum);
        }
        cum += c;
    }
    match tr.terminal {
        Terminal::Complete => End::Complete(cum),
        Terminal::Trap => End::Trap(cum),
        Terminal::Diverge => End::Exhaust(budget), // cost-1 charges fill the rest exactly
    }
}

// ---------------------------------------------------------------- reference

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Winner(usize), // branch index whose value is adopted
    ProgramTrap(usize),
    Exhausted,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct RaceResult {
    outcome: Outcome,
    /// Total fuel the race charges to enclosing regions. None when the
    /// program traps (no enclosing continuation observes anything).
    consumed: Option<u64>,
    /// The merge-order stream of charge events that semantically OCCUR:
    /// (post-time, branch, cost), strictly preceding the decisive event.
    /// This is what an enclosing bounded region sees streaming past.
    occurred: Option<Vec<(u64, usize, u64)>>,
}

/// Merge order on events: (time, branch index). All of branch i's events at
/// time t precede branch j's events at time t when i < j; within one branch,
/// trace order (charges before the terminal at equal time).
fn precedes(a: (u64, usize), b: (u64, usize)) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 < b.1)
}

fn reference(traces: &[Trace], budget: u64) -> RaceResult {
    let ends: Vec<End> = traces.iter().map(|t| run_branch(t, budget)).collect();
    // Decisive event: earliest (time, branch) among Complete and Trap ends.
    let decisive = ends
        .iter()
        .enumerate()
        .filter_map(|(j, e)| match e {
            End::Complete(t) => Some((*t, j, false)),
            End::Trap(t) => Some((*t, j, true)),
            End::Exhaust(_) => None,
        })
        .min_by_key(|&(t, j, _)| (t, j));

    // Charge events of branch j that occur: those with (post_time, j) strictly
    // preceding the decisive event — and never beyond the branch's own end.
    // The winner's charges all precede its own completion by within-branch
    // sequence order (equal time, same branch, charge-before-terminal), so it
    // contributes its full spend.
    let occurred_stream = |dec: Option<(u64, usize)>| -> Vec<(u64, usize, u64)> {
        let mut evs = Vec::new();
        for (j, tr) in traces.iter().enumerate() {
            let end_t = ends[j].time();
            let mut cum = 0u64;
            let push = |cum: u64, c: u64, evs: &mut Vec<(u64, usize, u64)>| {
                let occurs = match dec {
                    None => true,
                    // charge at (cum, j) occurs iff it precedes the decisive
                    // event, OR it belongs to the decisive branch itself
                    // (within-branch: its charges precede its terminal).
                    Some((dt, dj)) => precedes((cum, j), (dt, dj)) || j == dj,
                };
                if occurs {
                    evs.push((cum, j, c));
                }
            };
            for &c in &tr.charges {
                if cum + c > end_t {
                    break; // beyond this branch's own end (unaffordable charge)
                }
                cum += c;
                push(cum, c, &mut evs);
            }
            if tr.terminal == Terminal::Diverge {
                while cum < end_t {
                    cum += 1; // the diverging cost-1 tail up to the branch's end
                    push(cum, 1, &mut evs);
                }
            }
        }
        evs.sort_by_key(|&(t, j, _)| (t, j));
        evs
    };

    match decisive {
        Some((t, j, true)) => {
            // Program trap: nothing downstream observes fuel.
            let _ = t;
            let _ = j;
            RaceResult { outcome: Outcome::ProgramTrap(j), consumed: None, occurred: None }
        }
        Some((t, j, false)) => {
            let evs = occurred_stream(Some((t, j)));
            let consumed = evs.iter().map(|&(_, _, c)| c).sum();
            RaceResult {
                outcome: Outcome::Winner(j),
                consumed: Some(consumed),
                occurred: Some(evs),
            }
        }
        None => {
            let evs = occurred_stream(None);
            let consumed = evs.iter().map(|&(_, _, c)| c).sum();
            RaceResult { outcome: Outcome::Exhausted, consumed: Some(consumed), occurred: Some(evs) }
        }
    }
}

/// Theorem-3 cross-check: when the decisive event is a completion, it must be
/// the lexicographic minimum of (spend, index) over completed branches.
fn check_lexmin(traces: &[Trace], budget: u64, r: &RaceResult) {
    let ends: Vec<End> = traces.iter().map(|t| run_branch(t, budget)).collect();
    let lexmin = ends
        .iter()
        .enumerate()
        .filter_map(|(j, e)| match e {
            End::Complete(t) => Some((*t, j)),
            _ => None,
        })
        .min();
    if let Outcome::Winner(w) = r.outcome {
        let (_, lj) = lexmin.expect("winner exists but no completion");
        // The lexmin completion can differ from the winner ONLY if a trap
        // preempted — but then the outcome would be ProgramTrap. So equality.
        assert_eq!(w, lj, "winner must be the (spend, index) lexmin completion");
    }
}

// ------------------------------------------------------- capped branch runs

/// What a capped run of one branch reveals: the charges it managed to take
/// (post-times and costs), and how it stopped. `Parked` = the cap (not the
/// branch budget) blocked the next charge; the branch's true end is unknown.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Revealed {
    taken: Vec<(u64, u64)>, // (post_time, cost)
    stop: Stop,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stop {
    Complete(u64),
    Trap(u64),
    Exhaust(u64), // the branch's OWN budget blocked it (a real End)
    Parked(u64),  // the cap blocked it; consumed this much so far
}

fn run_capped(tr: &Trace, budget: u64, cap: u64) -> Revealed {
    let mut cum = 0u64;
    let mut taken = Vec::new();
    for &c in &tr.charges {
        if cum + c > budget {
            return Revealed { taken, stop: Stop::Exhaust(cum) };
        }
        if cum + c > cap {
            return Revealed { taken, stop: Stop::Parked(cum) };
        }
        cum += c;
        taken.push((cum, c));
    }
    match tr.terminal {
        Terminal::Complete => Revealed { taken, stop: Stop::Complete(cum) },
        Terminal::Trap => Revealed { taken, stop: Stop::Trap(cum) },
        Terminal::Diverge => {
            // cost-1 charges until budget or cap intervenes
            let stop_at = budget.min(cap);
            while cum < stop_at {
                cum += 1;
                taken.push((cum, 1));
            }
            if stop_at == budget && budget <= cap {
                Revealed { taken, stop: Stop::Exhaust(cum) }
            } else {
                Revealed { taken, stop: Stop::Parked(cum) }
            }
        }
    }
}

/// Decide the outcome + reconstruct the occurred stream from revealed data
/// ONLY (no peeking at the traces). Shared by SEQ and ADV.
fn decide(revealed: &[Revealed]) -> RaceResult {
    let decisive = revealed
        .iter()
        .enumerate()
        .filter_map(|(j, r)| match r.stop {
            Stop::Complete(t) => Some((t, j, false)),
            Stop::Trap(t) => Some((t, j, true)),
            _ => None,
        })
        .min_by_key(|&(t, j, _)| (t, j));

    let stream = |dec: Option<(u64, usize)>| -> Vec<(u64, usize, u64)> {
        let mut evs = Vec::new();
        for (j, r) in revealed.iter().enumerate() {
            for &(t, c) in &r.taken {
                let occurs = match dec {
                    None => true,
                    Some((dt, dj)) => precedes((t, j), (dt, dj)) || j == dj,
                };
                if occurs {
                    evs.push((t, j, c));
                }
            }
        }
        evs.sort_by_key(|&(t, j, _)| (t, j));
        evs
    };

    match decisive {
        Some((_, j, true)) => {
            RaceResult { outcome: Outcome::ProgramTrap(j), consumed: None, occurred: None }
        }
        Some((t, j, false)) => {
            let evs = stream(Some((t, j)));
            let consumed = evs.iter().map(|&(_, _, c)| c).sum();
            RaceResult { outcome: Outcome::Winner(j), consumed: Some(consumed), occurred: Some(evs) }
        }
        None => {
            let evs = stream(None);
            let consumed = evs.iter().map(|&(_, _, c)| c).sum();
            RaceResult { outcome: Outcome::Exhausted, consumed: Some(consumed), occurred: Some(evs) }
        }
    }
}

// ---------------------------------------------------------------- SEQ scan

/// The wasm lowering strategy: branches in list order, each under
/// cap = min(budget, d_time - 1) where d is the earliest decisive candidate
/// recorded so far (completion or trap). All later branches have index greater
/// than every recorded index, so the -1 form is always the sound cap.
fn sequential_scan(traces: &[Trace], budget: u64) -> RaceResult {
    let mut revealed: Vec<Revealed> = Vec::with_capacity(traces.len());
    let mut earliest: Option<(u64, usize)> = None; // decisive candidate (time, idx)
    for (j, tr) in traces.iter().enumerate() {
        let cap = match earliest {
            None => budget,
            Some((t, _)) => budget.min(t.saturating_sub(1)),
        };
        let r = run_capped(tr, budget, cap);
        match r.stop {
            Stop::Complete(t) | Stop::Trap(t) => {
                let cand = (t, j);
                if earliest.map_or(true, |e| precedes(cand, e)) {
                    earliest = Some(cand);
                }
            }
            _ => {}
        }
        revealed.push(r);
    }
    decide(&revealed)
}

// ------------------------------------------------------------ ADV schedules

/// Adversarial parallel implementation. State: how far each branch has been
/// physically advanced. The adversary picks any branch that can still take a
/// step under the CURRENT cap (caps derive from decisive candidates recorded
/// so far, so they depend on the schedule — that is exactly what the
/// confluence check must survive). Pruning is applied exactly at the cap.
///
/// cap_k = min(budget, d_time - (k > d_idx ? 1 : 0)) for the earliest
/// recorded decisive candidate d; no cap before any candidate exists.
///
/// The adversary additionally models LAZY cap checks: when the cap (but not
/// the branch budget) would block the next charge, it may either park the
/// branch or overrun — take the charge anyway, as a real implementation whose
/// fuel check is periodic would. decide() must be robust to the extra
/// revealed data.
struct Adv<'a> {
    traces: &'a [Trace],
    budget: u64,
    outcomes: BTreeSet<String>,
    seen: HashSet<Vec<u64>>,
    /// Guard so the exploration stays finite even if a bug makes it grow.
    states_explored: u64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum BPos {
    Running { next: usize, cum: u64 }, // next charge index (or terminal if past)
    Done(Stop),
}

impl<'a> Adv<'a> {
    fn cap_for(&self, k: usize, earliest: Option<(u64, usize)>) -> u64 {
        match earliest {
            None => self.budget,
            Some((t, dj)) => {
                let thr = if k > dj { t.saturating_sub(1) } else { t };
                self.budget.min(thr)
            }
        }
    }

    /// One physical step of branch k. Returns false if the branch cannot step
    /// (parked or done) — the caller then marks it Done(Parked).
    fn explore(&mut self, pos: &mut Vec<BPos>) {
        self.states_explored += 1;
        assert!(self.states_explored < 200_000_000, "state explosion");
        // Memoize on a canonical encoding of the state.
        let key: Vec<u64> = pos
            .iter()
            .map(|p| match p {
                BPos::Running { next, cum } => (*next as u64) << 32 | cum,
                BPos::Done(Stop::Complete(t)) => 1 << 60 | t,
                BPos::Done(Stop::Trap(t)) => 2 << 60 | t,
                BPos::Done(Stop::Exhaust(t)) => 3 << 60 | t,
                BPos::Done(Stop::Parked(t)) => 4 << 60 | t,
            })
            .collect();
        if !self.seen.insert(key) {
            return;
        }

        // Earliest decisive candidate among branches already Done.
        let earliest = pos
            .iter()
            .enumerate()
            .filter_map(|(j, p)| match p {
                BPos::Done(Stop::Complete(t)) | BPos::Done(Stop::Trap(t)) => Some((*t, j)),
                _ => None,
            })
            .min();

        let mut any_step = false;
        for k in 0..pos.len() {
            let BPos::Running { next, cum } = pos[k].clone() else { continue };
            let cap = self.cap_for(k, earliest);
            let tr = &self.traces[k];
            // What can branch k's next physical step do? Usually one option;
            // at the cap boundary the adversary chooses park OR overrun.
            let mut options: Vec<BPos> = Vec::with_capacity(2);
            if next < tr.charges.len() {
                let c = tr.charges[next];
                if cum + c > self.budget {
                    options.push(BPos::Done(Stop::Exhaust(cum)));
                } else if cum + c > cap {
                    options.push(BPos::Done(Stop::Parked(cum)));
                    options.push(BPos::Running { next: next + 1, cum: cum + c }); // overrun
                } else {
                    options.push(BPos::Running { next: next + 1, cum: cum + c });
                }
            } else {
                match tr.terminal {
                    Terminal::Complete => options.push(BPos::Done(Stop::Complete(cum))),
                    Terminal::Trap => options.push(BPos::Done(Stop::Trap(cum))),
                    Terminal::Diverge => {
                        if cum + 1 > self.budget {
                            options.push(BPos::Done(Stop::Exhaust(cum)));
                        } else if cum + 1 > cap {
                            options.push(BPos::Done(Stop::Parked(cum)));
                            options.push(BPos::Running { next, cum: cum + 1 }); // overrun
                        } else {
                            options.push(BPos::Running { next, cum: cum + 1 });
                        }
                    }
                }
            }
            any_step = true;
            for stepped in options {
                let saved = std::mem::replace(&mut pos[k], stepped);
                self.explore(pos);
                pos[k] = saved;
            }
        }

        if !any_step {
            // Terminal schedule state: decide from revealed data.
            let revealed: Vec<Revealed> = pos
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    let BPos::Done(stop) = p else { unreachable!() };
                    // Reconstruct taken charges from the trace prefix — this
                    // mirrors what a real implementation recorded while
                    // stepping; it is revealed data, not peeking.
                    let tr = &self.traces[j];
                    let mut taken = Vec::new();
                    let mut cum = 0u64;
                    let stop_t = match stop {
                        Stop::Complete(t) | Stop::Trap(t) | Stop::Exhaust(t) | Stop::Parked(t) => *t,
                    };
                    for &c in &tr.charges {
                        if cum + c > stop_t {
                            break;
                        }
                        cum += c;
                        taken.push((cum, c));
                    }
                    while cum < stop_t {
                        cum += 1; // diverging cost-1 tail
                        taken.push((cum, 1));
                    }
                    Revealed { taken, stop: *stop }
                })
                .collect();
            let r = decide(&revealed);
            self.outcomes.insert(format!("{r:?}"));
        }
    }
}

fn adversarial_outcomes(traces: &[Trace], budget: u64) -> BTreeSet<String> {
    let mut adv = Adv {
        traces,
        budget,
        outcomes: BTreeSet::new(),
        seen: HashSet::new(),
        states_explored: 0,
    };
    let mut pos = vec![BPos::Running { next: 0, cum: 0 }; traces.len()];
    adv.explore(&mut pos);
    adv.outcomes
}

// ------------------------------------------------------------------ nesting

/// Outer bounded region with budget `m`: the race's occurred stream drains it
/// in merge order; outer exhaustion mid-stream abandons the race. Afterwards
/// one extra outer charge of cost `extra` (0 = none) must still be affordable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OuterOutcome {
    Ok { outer_left: u64, winner: usize },
    RaceExhaustedErr { outer_left: u64 },
    OuterExhausted,
    ProgramTrap(usize),
}

fn outer_semantics(race: &RaceResult, m: u64, extra: u64) -> OuterOutcome {
    match &race.outcome {
        Outcome::ProgramTrap(j) => OuterOutcome::ProgramTrap(*j),
        _ => {
            let mut left = m;
            for &(_, _, c) in race.occurred.as_ref().unwrap() {
                if c > left {
                    return OuterOutcome::OuterExhausted;
                }
                left -= c;
            }
            // Race settled within the outer budget.
            if extra > left {
                return OuterOutcome::OuterExhausted;
            }
            left -= extra;
            match race.outcome {
                Outcome::Winner(j) => OuterOutcome::Ok { outer_left: left, winner: j },
                Outcome::Exhausted => OuterOutcome::RaceExhaustedErr { outer_left: left },
                Outcome::ProgramTrap(_) => unreachable!(),
            }
        }
    }
}

// --------------------------------------------------------------------- main

fn all_traces(max_charges: usize) -> Vec<Trace> {
    let mut charge_seqs: Vec<Vec<u64>> = vec![vec![]];
    let mut frontier: Vec<Vec<u64>> = vec![vec![]];
    for _ in 0..max_charges {
        let mut next = Vec::new();
        for s in &frontier {
            for c in [1u64, 2] {
                let mut s2 = s.clone();
                s2.push(c);
                next.push(s2);
            }
        }
        charge_seqs.extend(next.iter().cloned());
        frontier = next;
    }
    let mut out = Vec::new();
    for s in charge_seqs {
        for t in [Terminal::Complete, Terminal::Trap, Terminal::Diverge] {
            out.push(Trace { charges: s.clone(), terminal: t });
        }
    }
    out
}

fn main() {
    let mut configs = 0u64;
    let mut adv_configs = 0u64;

    // Scope A: k <= 3 branches, <= 2 charges each, budgets 0..=5.
    // Scope B (wider traces, fewer branches): k <= 2, <= 3 charges, 0..=7.
    let scopes: Vec<(usize, usize, u64)> = vec![(3, 2, 5), (2, 3, 7)];

    for (max_k, max_charges, max_budget) in scopes {
        let univ = all_traces(max_charges);
        let mut stack: Vec<Vec<usize>> = vec![vec![]];
        while let Some(sel) = stack.pop() {
            if sel.len() >= 1 {
                let traces: Vec<Trace> = sel.iter().map(|&i| univ[i].clone()).collect();
                for budget in 0..=max_budget {
                    configs += 1;
                    let r = reference(&traces, budget);
                    check_lexmin(&traces, budget, &r);

                    // SEQ must equal REF, including consumed + occurred stream.
                    let s = sequential_scan(&traces, budget);
                    assert_eq!(s, r, "SEQ != REF for {traces:?} budget {budget}");

                    // ADV confluence: every schedule reaches exactly REF.
                    // (Bounded to k <= 3; the state space is memoized.)
                    adv_configs += 1;
                    let outs = adversarial_outcomes(&traces, budget);
                    let want: BTreeSet<String> = [format!("{r:?}")].into();
                    assert_eq!(
                        outs, want,
                        "ADV outcomes diverge for {traces:?} budget {budget}"
                    );

                    // Nesting: outer streaming outcome computed from REF's
                    // occurred stream must equal the one computed from SEQ's
                    // reconstructed stream (they are asserted equal above, so
                    // this checks the outer arithmetic is well-defined on it).
                    if !matches!(r.outcome, Outcome::ProgramTrap(_)) {
                        for m in 0..=(max_budget + 2) {
                            for extra in [0u64, 1] {
                                let a = outer_semantics(&r, m, extra);
                                let b = outer_semantics(&s, m, extra);
                                assert_eq!(a, b, "outer divergence");
                            }
                        }
                    }
                }
            }
            if sel.len() < max_k {
                let start = 0; // ordered tuples: branch order matters
                for i in start..univ.len() {
                    let mut s2 = sel.clone();
                    s2.push(i);
                    stack.push(s2);
                }
            }
        }
        println!(
            "scope k<={max_k} charges<={max_charges} budget<={max_budget}: OK"
        );
    }

    println!("configs checked: {configs} (adversarial confluence on {adv_configs})");
    println!("ALL CHECKS PASSED");
}
