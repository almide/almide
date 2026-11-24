//! The runtime routines the proof script transcribes, held to the bodies
//! that ship.
//!
//! `proofs/StructuralDecode.v` carries the instruction sequences of four
//! routines (`inc_bytes`, `dec_bytes`, `free_bytes`, `alloc_bytes`) and
//! proves that they decode to the trees its realization theorems are about.
//! `proofs/check-structural-bytes.sh` holds the emitter's output to those
//! lists. A module ships after two more passes (the stock-WASI transform and
//! the prune), so the lists are read here and compared with the shipped
//! bodies, and every difference those passes may make is spelled out:
//!
//! - a function index or a global index may be renumbered, the same way at
//!   every place in the four bodies (the proof's decoder takes them from a
//!   naming);
//! - the constant in front of the out-of-memory report's call may be any
//!   address (the proof's decoder takes that immediate as a parameter; the
//!   lists carry the probe value `0` there);
//! - `i32.const 1; call <the imported proc_exit>` may stand in front of an
//!   `unreachable` (the stock-WASI transform puts it in front of every one).
//!
//! Any other difference refuses the module. The four bodies are the function
//! the anchor file names as `alloc`, the `rcfns` line's rc_inc, and the two
//! its `proved` line names (the flat release, then the free routine); the
//! flat release's one call must name that free routine.
//!
//! TRUSTED: the two callees of `$alloc` no body is given for (the
//! out-of-memory report and the exit); that the lists in the proof script
//! are the ones its theorems are about (the Rocq kernel re-checks that on
//! every build).

use std::collections::HashMap;

use wasmparser::{BinaryReader, Operator as Op, OperatorsReader, Parser, Payload, TypeRef};

use crate::decode::Body;

const PROOF: &str = include_str!("../../../proofs/StructuralDecode.v");

/// The anchor file's `proved` line: the flat release and the free routine, as
/// function indices of the shipped module (`None`: the routine did not ship).
pub type ProvedLine = (Option<u32>, Option<u32>);

/// The bytes of the proof script's `<name>_bytes` list.
fn list(name: &str) -> Result<Vec<u8>, String> {
    let head = format!("Definition {name}_bytes : list Z :=");
    let rest = PROOF.split_once(head.as_str()).map(|(_, r)| r).ok_or_else(|| format!("proved: the proof script has no `{name}_bytes`"))?;
    let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) else {
        return Err(format!("proved: `{name}_bytes` is not a bracketed list"));
    };
    rest[open + 1..close]
        .split(';')
        .map(|t| t.trim().parse::<u8>().map_err(|_| format!("proved: `{name}_bytes` holds `{}`, which is not a byte", t.trim())))
        .collect()
}

/// The instructions `bytes` encode (a body without its locals vector).
fn instructions(bytes: &[u8]) -> Result<Vec<Op<'_>>, String> {
    let mut r = OperatorsReader::new(BinaryReader::new(bytes, 0));
    let mut out = Vec::new();
    while !r.eof() {
        out.push(r.read().map_err(|e| format!("proved: the proof script's bytes do not decode: {e}"))?);
    }
    Ok(out)
}

/// How the shipped module renumbers what the proof's lists name, and the
/// import the transform's exit calls name.
#[derive(Default)]
struct Renames {
    funcs: HashMap<u32, u32>,
    globals: HashMap<u32, u32>,
    exit: Option<u32>,
}

/// `proved` is `shipped` under `map`: the same answer as every earlier place,
/// and no two proved indices on one shipped index.
fn renamed(map: &mut HashMap<u32, u32>, proved: u32, shipped: u32) -> bool {
    match map.get(&proved) {
        Some(&s) => s == shipped,
        None => {
            if map.values().any(|&s| s == shipped) {
                return false;
            }
            map.insert(proved, shipped);
            true
        }
    }
}

/// `shipped` is the proof's `proved`, up to the three differences of the
/// module header.
fn held(name: &str, shipped: &[Op<'_>], proved: &[Op<'_>], imports: u32, r: &mut Renames) -> Result<(), String> {
    let differs = |j: usize, i: usize, what: String| format!("proved: `${name}` is not the proof's `{name}_bytes` at instruction {j} of the list ({i} of the shipped body): {what}");
    let mut i = 0;
    for (j, p) in proved.iter().enumerate() {
        // The stock-WASI transform's exit call in front of a trap.
        if matches!(p, Op::Unreachable) {
            if let (Some(Op::I32Const { value: 1 }), Some(Op::Call { function_index })) = (shipped.get(i), shipped.get(i + 1)) {
                if *function_index >= imports {
                    return Err(differs(j, i, format!("the call in front of the trap names function {function_index}, which is not an import")));
                }
                if *r.exit.get_or_insert(*function_index) != *function_index {
                    return Err(differs(j, i, format!("the call in front of the trap names import {function_index}, another names {:?}", r.exit)));
                }
                i += 2;
            }
        }
        let Some(s) = shipped.get(i) else {
            return Err(differs(j, i, "the shipped body ends".to_string()));
        };
        let same = match (s, p) {
            (Op::Call { function_index: a }, Op::Call { function_index: b }) => renamed(&mut r.funcs, *b, *a),
            (Op::GlobalGet { global_index: a }, Op::GlobalGet { global_index: b }) | (Op::GlobalSet { global_index: a }, Op::GlobalSet { global_index: b }) => {
                renamed(&mut r.globals, *b, *a)
            }
            // The out-of-memory report's argument: the probe immediate.
            (Op::I32Const { .. }, Op::I32Const { value: 0 }) if matches!(proved.get(j + 1), Some(Op::Call { .. })) => true,
            (a, b) => a == b,
        };
        if !same {
            return Err(differs(j, i, format!("{s:?} where the list has {p:?}")));
        }
        i += 1;
    }
    if i != shipped.len() {
        return Err(format!("proved: `${name}` has {} instruction(s) after the last of the proof's `{name}_bytes`", shipped.len() - i));
    }
    Ok(())
}

/// The field names of the module's function imports, by function index.
fn import_names(wasm: &[u8]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        if let Payload::ImportSection(r) = payload.map_err(|e| e.to_string())? {
            for i in r.into_imports() {
                let i = i.map_err(|e| e.to_string())?;
                if matches!(i.ty, TypeRef::Func(_)) {
                    out.push(i.name.to_string());
                }
            }
        }
    }
    Ok(out)
}

/// Hold the routines the anchor file names to the proof script's lists:
/// `alloc`, the rc_inc `inc`, and the `proved` line's flat release and free
/// routine. Answers how many were held. `imports` is the module's count of
/// imported functions.
pub fn check_proved_routines(
    wasm: &[u8],
    bodies: &[Body<'_>],
    imports: u32,
    alloc: Option<u32>,
    inc: Option<u32>,
    line: ProvedLine,
) -> Result<usize, String> {
    let (dec, free) = line;
    let mut r = Renames::default();
    let mut n = 0;
    for (name, f) in [("alloc", alloc), ("inc", inc), ("dec", dec), ("free", free)] {
        let Some(f) = f else { continue };
        let b = f
            .checked_sub(imports)
            .and_then(|d| bodies.get(d as usize))
            .ok_or_else(|| format!("proved: `${name}` is named as function {f}, which the module does not define"))?;
        let bytes = list(name)?;
        held(name, &b.ops, &instructions(&bytes)?, imports, &mut r)?;
        n += 1;
    }
    // The flat release frees through the routine held as `$free`.
    if dec.is_some() {
        let bytes = list("dec")?;
        let callee = instructions(&bytes)?.iter().find_map(|o| match o {
            Op::Call { function_index } => Some(*function_index),
            _ => None,
        });
        let through = callee.and_then(|c| r.funcs.get(&c).copied());
        if through.is_none() || through != free {
            return Err(format!("proved: the flat release frees through function {through:?}, and the anchor file names {free:?} as the free routine"));
        }
    }
    if let Some(x) = r.exit {
        let names = import_names(wasm)?;
        match names.get(x as usize).map(String::as_str) {
            Some("proc_exit") => {}
            other => return Err(format!("proved: the call in front of a proved routine's trap names import {x} ({other:?}), not `proc_exit`")),
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The proof's own list as the shipped body, after `edit`, against the
    /// list; `imports` imported functions, a fresh renaming.
    fn against(name: &str, imports: u32, edit: impl FnOnce(&mut Vec<Op<'_>>)) -> Result<(), String> {
        let bytes = list(name).unwrap();
        let proved = instructions(&bytes).unwrap();
        let mut shipped = proved.clone();
        edit(&mut shipped);
        held(name, &shipped, &proved, imports, &mut Renames::default())
    }

    fn at(ops: &[Op<'_>], want: impl Fn(&Op<'_>) -> bool) -> usize {
        ops.iter().position(want).expect("the list holds the instruction")
    }

    #[test]
    fn the_lists_have_the_lengths_the_proof_states() {
        for (name, n) in [("inc", 13), ("dec", 27), ("free", 108), ("alloc", 258)] {
            assert_eq!(instructions(&list(name).unwrap()).unwrap().len(), n, "{name}");
        }
    }

    #[test]
    fn a_list_is_held_to_itself() {
        for name in ["inc", "dec", "free", "alloc"] {
            against(name, 0, |_| {}).unwrap();
        }
    }

    #[test]
    fn renumbered_calls_and_globals_are_held() {
        against("dec", 0, |s| {
            for op in s.iter_mut() {
                match op {
                    Op::Call { function_index } => *function_index = 12,
                    Op::GlobalGet { global_index } => *global_index = 1,
                    _ => {}
                }
            }
        })
        .unwrap();
    }

    /// `i32.const 1; call f` in front of the body's first trap.
    fn exit_call(s: &mut Vec<Op<'_>>, f: u32) {
        let trap = at(s, |o| matches!(o, Op::Unreachable));
        s.insert(trap, Op::Call { function_index: f });
        s.insert(trap, Op::I32Const { value: 1 });
    }

    /// `ops` with every global read renumbered to `g`.
    fn reading_global<'a>(ops: &[Op<'a>], g: u32) -> Vec<Op<'a>> {
        ops.iter()
            .cloned()
            .map(|o| match o {
                Op::GlobalGet { .. } => Op::GlobalGet { global_index: g },
                o => o,
            })
            .collect()
    }

    #[test]
    fn the_exit_call_in_front_of_a_trap_is_held_only_for_an_import() {
        against("dec", 2, |s| exit_call(s, 1)).unwrap();
        assert!(against("dec", 2, |s| exit_call(s, 7)).unwrap_err().contains("not an import"));
    }

    #[test]
    fn a_changed_constant_is_refused() {
        let e = against("dec", 0, |s| {
            let k = at(s, |o| matches!(o, Op::I32Const { value: 1 }));
            s[k] = Op::I32Const { value: 2 };
        })
        .unwrap_err();
        assert!(e.contains("is not the proof's `dec_bytes`"), "{e}");
    }

    #[test]
    fn a_dropped_instruction_is_refused() {
        assert!(against("inc", 0, |s| {
            let k = at(s, |o| matches!(o, Op::I32Add));
            s.remove(k);
        })
        .is_err());
        assert!(against("free", 0, |s| {
            s.pop();
        })
        .is_err());
    }

    #[test]
    fn an_instruction_after_the_last_is_refused() {
        assert!(against("inc", 0, |s| s.push(Op::Nop)).unwrap_err().contains("after the last"));
    }

    #[test]
    fn the_out_of_memory_address_may_differ_and_the_exit_code_may_not() {
        // The report's argument is the probe immediate `0` in front of a call.
        against("alloc", 0, |s| {
            for k in 0..s.len() - 1 {
                if matches!((&s[k], &s[k + 1]), (Op::I32Const { value: 0 }, Op::Call { .. })) {
                    s[k] = Op::I32Const { value: 4096 };
                }
            }
        })
        .unwrap();
        assert!(against("alloc", 0, |s| {
            let k = (0..s.len() - 1).find(|&k| matches!((&s[k], &s[k + 1]), (Op::I32Const { value: 1 }, Op::Call { .. }))).unwrap();
            s[k] = Op::I32Const { value: 0 };
        })
        .is_err());
    }

    #[test]
    fn one_renaming_serves_all_four_bodies() {
        // `$inc` and the flat release read the same global: a module that
        // renumbers it one way in one body and another way in the other is
        // refused.
        let (ib, db) = (list("inc").unwrap(), list("dec").unwrap());
        let (inc, dec) = (instructions(&ib).unwrap(), instructions(&db).unwrap());
        let mut r = Renames::default();
        held("inc", &reading_global(&inc, 1), &inc, 0, &mut r).unwrap();
        held("dec", &reading_global(&dec, 1), &dec, 0, &mut r).unwrap();
        let mut r = Renames::default();
        held("inc", &reading_global(&inc, 1), &inc, 0, &mut r).unwrap();
        assert!(held("dec", &reading_global(&dec, 2), &dec, 0, &mut r).is_err());
    }
}
