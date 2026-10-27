//! #3908 family gate (R-env-02): the environment-variable family of `env` is
//! a MATRIX, never a point. It is the four roles {read one, read all, write,
//! unset} — `get`, `vars`, `set`, `remove` — each with exactly the declared
//! signature. `set` and `remove` are the fallible pair: both refuse a name no
//! environment can hold with an err (#4200), so both return
//! `Result[Unit, String]`.
//!
//! Intentional omissions (shrink-only — a name may leave this list only by
//! becoming a cell of the matrix, with its spec coverage, in the same PR):
//! the other languages' spellings of the same four roles (`var`, `environ`,
//! `unset`, `delete`, `put`), which would give one role a second name
//! (R-L0-01). Any other public `env` fn whose name says "variable" fails here
//! too, so the family cannot grow point-wise.
//!
//! The rule is a pure function over the module text so the negative control
//! below can run it on a text with one cell removed and see it fail.

fn env_source() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/env.almd");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The four cells, each with its declared signature.
const CELLS: &[&str] = &[
    "effect fn get(name: String) -> String?",
    "effect fn vars() -> Map[String, String]",
    "effect fn set(name: String, value: String) -> Result[Unit, String]",
    "effect fn remove(name: String) -> Result[Unit, String]",
];

/// Shrink-only: the same roles under other languages' names.
const INTENTIONALLY_OMITTED: &[&str] = &["var", "environ", "unset", "delete", "put", "get_env", "set_var", "remove_var"];

/// The family rule over `env.almd`: every cell declared with its signature,
/// no omitted spelling declared as a public fn.
fn family_problems(src: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for cell in CELLS {
        if !src.lines().any(|l| l.starts_with(cell)) {
            problems.push(format!("cell missing: {cell}"));
        }
    }
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("effect fn ").or_else(|| line.strip_prefix("fn ")) else { continue };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if INTENTIONALLY_OMITTED.contains(&name.as_str()) {
            problems.push(format!("`env.{name}` is another spelling of a family role (R-env-02, R-L0-01)"));
        }
    }
    problems
}

#[test]
fn the_env_variable_family_is_get_vars_set_remove() {
    let problems = family_problems(&env_source());
    assert!(
        problems.is_empty(),
        "R-env-02: the environment-variable family is {{get, vars, set, remove}} (update the rule in \
         docs/roadmap/stdlib-redesign/L2-env.md and this gate in the same PR):\n{}",
        problems.join("\n")
    );
}

/// Negative control: the gate must FAIL on a text with one cell removed, with
/// a cell's signature changed, and with an omitted spelling added.
#[test]
fn control_the_gate_fails_when_a_cell_is_dropped_or_respelled() {
    let src = env_source();
    let dropped = src.replace("effect fn remove(", "effect fn removed_cell(");
    assert!(
        family_problems(&dropped).iter().any(|p| p.contains("fn remove(")),
        "control: dropping a cell must fail the gate"
    );
    let unit_remove = src.replace(
        "effect fn remove(name: String) -> Result[Unit, String]",
        "effect fn remove(name: String) -> Unit",
    );
    assert!(!family_problems(&unit_remove).is_empty(), "control: a changed signature must fail the gate");
    let respelled = format!("{src}\neffect fn unset(name: String) -> Unit = _\n");
    assert!(
        family_problems(&respelled).iter().any(|p| p.contains("env.unset")),
        "control: another spelling of a role must fail the gate"
    );
}
