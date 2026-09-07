//! #1423 stage 5, first step (ruling 2026-09-04): the campaign summary names
//! the walls that contradict the target-availability declaration — a wall
//! on a fn the matrix declares AVAILABLE on the stock-p1 leg — as a WARNING
//! histogram, not a finding. The promotion to a finding class waits for
//! stage 4's pending-self-host rows to reach zero; until then every such
//! line is a frontier row the declaration cannot see (the probe measures a
//! minimal call per public fn, the fuzzer hits the typed twins).
//!
//! The wall reasons that name functions are the `unlinked stdlib/runtime
//! call(s) with no wasm definition: <names> — …` class, and the names are
//! the lowering's typed twins (`list.zip_with_x`, `map.from_list_hval_wall`,
//! `list.take_while_heapelem`). A twin maps to its public fn by stripping
//! trailing `_segment`s until the name is one the module interface exports
//! (`almide compile <module> --json`, the same enumeration the availability
//! probe uses) — never a fixed suffix list, which would rot with every new
//! twin.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

const UNLINKED_PREFIX: &str = "unlinked stdlib/runtime call(s) with no wasm definition: ";

/// The fn names a wall reason carries, or none for every other reason class.
pub fn unlinked_names(reason: &str) -> Vec<String> {
    let Some(rest) = reason.strip_prefix(UNLINKED_PREFIX) else { return Vec::new() };
    let names = rest.split(" —").next().unwrap_or(rest);
    names
        .split(',')
        .map(|s| s.trim().trim_end_matches('…').to_string())
        .filter(|s| s.contains('.'))
        .collect()
}

/// A typed twin's public fn: strip trailing `_segment`s until `public`
/// knows the name; a name that never resolves is returned as is.
pub fn public_of_twin(name: &str, public: &BTreeSet<String>) -> String {
    let Some((module, mut func)) = name.split_once('.') else { return name.to_string() };
    loop {
        let candidate = format!("{module}.{func}");
        if public.contains(&candidate) {
            return candidate;
        }
        match func.rfind('_') {
            Some(i) if i > 0 => func = &func[..i],
            _ => return name.to_string(),
        }
    }
}

/// `fn -> legs` for every `[[unavailable]]` row of the availability matrix
/// — the three keys this file needs, read with the same line discipline
/// scripts/check-target-availability.sh applies (a `[[unavailable]]` header
/// opens a row; `fn = "…"` and `legs = […]` fill it).
pub fn unavailable_rows(toml: &str) -> HashMap<String, Vec<String>> {
    let mut rows: HashMap<String, Vec<String>> = HashMap::new();
    let (mut cur_fn, mut cur_legs): (Option<String>, Vec<String>) = (None, Vec::new());
    let flush = |f: &mut Option<String>, l: &mut Vec<String>, rows: &mut HashMap<String, Vec<String>>| {
        if let Some(name) = f.take() {
            rows.entry(name).or_default().append(l);
        }
        l.clear();
    };
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with("[[") {
            flush(&mut cur_fn, &mut cur_legs, &mut rows);
            continue;
        }
        if let Some(v) = line.strip_prefix("fn = ") {
            cur_fn = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("legs = ") {
            cur_legs = v
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    flush(&mut cur_fn, &mut cur_legs, &mut rows);
    rows
}

/// The public fns of `module` per the compiler's own interface, or an empty
/// set when the module has none (an unknown module, a compile failure).
fn public_fns(almide: &Path, module: &str) -> BTreeSet<String> {
    let out = Command::new(almide).args(["compile", module, "--json"]).output();
    let Ok(out) = out else { return BTreeSet::new() };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return BTreeSet::new();
    };
    v["functions"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter_map(|f| f["name"].as_str())
                .map(|n| format!("{module}.{n}"))
                .collect()
        })
        .unwrap_or_default()
}

/// `public fn -> wall count` for the walls whose named fn the matrix does
/// NOT declare unavailable on `leg` — the stage-5 candidates.
pub fn declared_available_walls(
    reasons: &BTreeMap<String, u64>,
    repo: &Path,
    almide: &Path,
    leg: &str,
) -> BTreeMap<String, u64> {
    let toml = std::fs::read_to_string(repo.join("proofs/target-availability.toml")).unwrap_or_default();
    let unavailable = unavailable_rows(&toml);
    let mut public_cache: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    for (reason, n) in reasons {
        for name in unlinked_names(reason) {
            let module = name.split('.').next().unwrap_or("").to_string();
            let public = public_cache
                .entry(module.clone())
                .or_insert_with(|| public_fns(almide, &module));
            let public_name = public_of_twin(&name, public);
            if !public.contains(&public_name) {
                continue; // no public fn behind the twin: not the declaration's subject
            }
            let declared = unavailable
                .get(&public_name)
                .is_some_and(|legs| legs.iter().any(|l| l == leg));
            if !declared {
                *out.entry(public_name).or_insert(0) += n;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlinked_names_come_from_the_one_reason_class() {
        let r = "unlinked stdlib/runtime call(s) with no wasm definition: map.from_list_hval_wall, map.get_or_hval_wall — rendering them would emit a dangling `(call $…)`";
        assert_eq!(unlinked_names(r), vec!["map.from_list_hval_wall", "map.get_or_hval_wall"]);
        assert!(unlinked_names("main is outside the MIR-lowering subset: call argument Block").is_empty());
    }

    #[test]
    fn a_twin_resolves_to_its_public_fn_by_suffix_stripping() {
        let public: BTreeSet<String> =
            ["list.zip_with", "map.from_list", "list.take_while"].iter().map(|s| s.to_string()).collect();
        assert_eq!(public_of_twin("list.zip_with_x", &public), "list.zip_with");
        assert_eq!(public_of_twin("map.from_list_hval_wall", &public), "map.from_list");
        assert_eq!(public_of_twin("list.take_while_heapelem", &public), "list.take_while");
        assert_eq!(public_of_twin("list.nope_x", &public), "list.nope_x");
    }

    #[test]
    fn unavailable_rows_read_fn_and_legs() {
        let toml = "schema = 2\n\n[[unavailable]]\nfn = \"bytes.clear\"\nlegs = [\"structural\"]\nreason = \"pending-self-host\"\n\n[[unavailable]]\nfn = \"datetime.now\"\nlegs = [\"structural\", \"stock-p1\"]\n";
        let rows = unavailable_rows(toml);
        assert_eq!(rows["bytes.clear"], vec!["structural"]);
        assert_eq!(rows["datetime.now"], vec!["structural", "stock-p1"]);
    }
}
