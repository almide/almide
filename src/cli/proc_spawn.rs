//! Which bundled stdlib fns start a child process, and which argument names
//! the command (#4817) — read off what each fn REACHES, never a list of names.
//!
//! A runtime leaf that starts a child is declared `@spawns_process(<param>)`,
//! naming the parameter that carries the command. Every other bundled fn
//! spawns when its body calls one that spawns; its command is then the
//! argument it hands to that call's command position — one of its own
//! parameters, or a field of one (`run(c)` hands `c.cmd` to `__os_run`). A fn
//! that reaches a spawn but hands over anything else has no traceable command:
//! it is recorded as such, `[permissions] proc` refuses every call to it, and
//! `every_spawning_stdlib_fn_names_its_command` fails the build's tests, so a
//! new leaf can never be silently uncharged.

use std::collections::HashMap;
use std::sync::OnceLock;

use almide_lang::ast::{AttrValue, Decl, Expr, ExprKind, Program};

/// Where a spawning fn's call names its command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAt {
    /// Argument `i` is the command.
    Arg(usize),
    /// Argument `i` is a record; its field `f` is the command.
    Field(usize, String),
}

/// `(module, fn)` → its command position, or `None` when it reaches a spawn
/// but its command cannot be traced.
pub type SpawnTable = HashMap<(String, String), Option<CommandAt>>;

const MARK: &str = "spawns_process";

/// The table for the bundled stdlib.
pub fn spawning() -> &'static SpawnTable {
    static TABLE: OnceLock<SpawnTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let sources: Vec<(&str, String)> = almide_types::stdlib_info::BUNDLED_MODULES
            .iter()
            .filter_map(|&m| almide_types::stdlib_info::bundled_source(m).map(|s| (m, s.to_string())))
            .collect();
        spawning_of(&sources)
    })
}

struct FnInfo {
    params: Vec<String>,
    mark: Option<Option<String>>,
    body: Option<Expr>,
}

/// The table for `sources` (`(module, text)`): the leaves the mark names,
/// then every fn whose body reaches one, to a fixpoint.
pub fn spawning_of(sources: &[(&str, String)]) -> SpawnTable {
    let mut fns: Vec<((String, String), FnInfo)> = Vec::new();
    for (module, text) in sources {
        let tokens = crate::lexer::Lexer::tokenize(text);
        // A module that does not parse cannot be read for spawns: refuse
        // rather than leave its fns uncharged.
        let Ok(Program { decls, .. }) = crate::parser::Parser::new(tokens).parse() else {
            panic!("bundled stdlib module `{module}` does not parse; its process spawns cannot be read")
        };
        for d in decls {
            let Decl::Fn { name, attrs, params, body, .. } = d else { continue };
            let mark = attrs.iter().find(|a| a.name.as_str() == MARK).map(|a| {
                a.args.first().and_then(|arg| match &arg.value {
                    AttrValue::Ident { name } => Some(name.as_str().to_string()),
                    AttrValue::String { value } => Some(value.clone()),
                    _ => None,
                })
            });
            let params = params.iter().map(|p| p.name.as_str().to_string()).collect();
            fns.push(((module.to_string(), name.as_str().to_string()), FnInfo { params, mark, body }));
        }
    }
    let mut table: SpawnTable = HashMap::new();
    for (key, f) in &fns {
        if let Some(param) = &f.mark {
            let at = param.as_ref().and_then(|p| f.params.iter().position(|q| q == p)).map(CommandAt::Arg);
            table.insert(key.clone(), at);
        }
    }
    loop {
        let mut grew = false;
        for (key, f) in &fns {
            if table.contains_key(key) {
                continue;
            }
            let Some(body) = &f.body else { continue };
            if let Some(at) = reached(&key.0, &f.params, body, &table) {
                table.insert(key.clone(), at);
                grew = true;
            }
        }
        if !grew {
            return table;
        }
    }
}

/// When `body` (of a fn in `module` with `params`) calls a fn the table
/// knows to spawn: the command position it hands over, `None` if it cannot
/// be traced (or two such calls disagree). `Option::None` outside: no spawn
/// reached (yet).
fn reached(module: &str, params: &[String], body: &Expr, table: &SpawnTable) -> Option<Option<CommandAt>> {
    let mut found: Option<Option<CommandAt>> = None;
    // Every mention of a spawning fn, and how many of them are the callee of
    // a call: a mention that is not called (the fn passed as a value) hands
    // its command over where it cannot be traced.
    let mut mentions = 0usize;
    let mut called = 0usize;
    almide_lang::ast::visit_expr(body, &mut |e| {
        if spawn_target(module, e, table).is_some() {
            mentions += 1;
        }
        let ExprKind::Call { callee, args, .. } = &e.kind else { return };
        let Some(at) = spawn_target(module, callee, table) else { return };
        called += 1;
        let here = at.as_ref().and_then(|at| handed(params, args, at));
        found = Some(match &found {
            None => here,
            Some(prev) if *prev == here => here,
            Some(_) => None,
        });
    });
    if mentions > called {
        return Some(None);
    }
    found
}

/// The table's entry for the fn `e` names (`f` in `module`, or `m.f`).
fn spawn_target<'t>(module: &str, e: &Expr, table: &'t SpawnTable) -> Option<&'t Option<CommandAt>> {
    let key = match &e.kind {
        ExprKind::Ident { name } => (module.to_string(), name.as_str().to_string()),
        ExprKind::Member { object, field } => match &object.kind {
            ExprKind::Ident { name } => (name.as_str().to_string(), field.as_str().to_string()),
            _ => return None,
        },
        _ => return None,
    };
    table.get(&key)
}

/// The caller's command position, from the argument it passes at `at`.
fn handed(params: &[String], args: &[Expr], at: &CommandAt) -> Option<CommandAt> {
    let param = |e: &Expr| match &e.kind {
        ExprKind::Ident { name } => params.iter().position(|p| p == name.as_str()),
        _ => None,
    };
    match at {
        CommandAt::Arg(i) => {
            let a = args.get(*i)?;
            if let Some(p) = param(a) {
                return Some(CommandAt::Arg(p));
            }
            match &a.kind {
                ExprKind::Member { object, field } => Some(CommandAt::Field(param(object)?, field.as_str().to_string())),
                _ => None,
            }
        }
        CommandAt::Field(i, f) => Some(CommandAt::Field(param(args.get(*i)?)?, f.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate: every bundled fn that reaches a spawn names its command.
    #[test]
    fn every_spawning_stdlib_fn_names_its_command() {
        let t = spawning();
        let untraced: Vec<_> = t.iter().filter(|(_, at)| at.is_none()).map(|((m, f), _)| format!("{m}.{f}")).collect();
        assert!(untraced.is_empty(), "these stdlib fns reach a process spawn but hand it no traceable command: {untraced:?}");
        for f in ["exec", "output", "run", "start", "__os_run"] {
            assert!(t.contains_key(&("process".to_string(), f.to_string())), "process.{f} is not charged");
        }
        assert!(!t.contains_key(&("process".to_string(), "wait".to_string())));
    }

    /// The negative control: a new leaf with an odd signature is charged by
    /// its mark, a wrapper reaching it through a record field too, and a
    /// wrapper that computes the command is flagged untraceable.
    #[test]
    fn a_new_leaf_with_an_odd_signature_is_charged() {
        let src = "type Job = { exe: String, n: Int }\n\
            @spawns_process(program)\nlocal effect fn __odd(n: Int, program: String) -> Int = _\n\
            effect fn go(j: Job) -> Int = __odd(j.n, j.exe)\n\
            effect fn hidden(n: Int) -> Int = __odd(n, \"x\" + \"y\")\n\
            effect fn passed(n: Int) -> Int = apply(n, __odd)\n"
            .to_string();
        let t = spawning_of(&[("m", src)]);
        assert_eq!(t.get(&("m".into(), "__odd".into())), Some(&Some(CommandAt::Arg(1))));
        assert_eq!(t.get(&("m".into(), "go".into())), Some(&Some(CommandAt::Field(0, "exe".into()))));
        assert_eq!(t.get(&("m".into(), "hidden".into())), Some(&None), "a computed command must be flagged");
        assert_eq!(t.get(&("m".into(), "passed".into())), Some(&None), "a leaf passed as a value must be flagged");
    }
}
