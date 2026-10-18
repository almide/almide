//! Which parameters of a bundled stdlib function are CALLBACKS — declared
//! with a function type (`f: (B, A) -> B`) — read from the declaration in the
//! module's own source, the one place the signature lives.
//!
//! A position declared with a type VARIABLE (`list.fold`'s `init: B`,
//! `option.unwrap_or`'s `default: A`, `list.push`'s `x: T`) is a value
//! position even when the call instantiates it with a function type: the value
//! passes through and must come back as the same closure. A backend rewrite
//! meant for callbacks (the wasm leg's eta-expansion, which turns a fn-valued
//! variable into the literal lambda its higher-order arms inline) asks this
//! table instead of the argument's type, which cannot tell the two apart.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Per parameter, in declaration order: is it declared with a function type?
/// `None` when the module has no bundled source or does not declare `func`
/// at top level (the caller keeps its own conservative rule then).
pub fn callback_params(module: &str, func: &str) -> Option<Vec<bool>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), Option<Vec<bool>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (module.to_string(), func.to_string());
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let found = crate::stdlib_info::bundled_source(module).and_then(|src| scan(src, func));
    if let Ok(mut c) = cache.lock() {
        c.insert(key, found.clone());
    }
    found
}

/// The first top-level `[pub] [effect] fn <func>` declaration's parameter list.
fn scan(src: &str, func: &str) -> Option<Vec<bool>> {
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let rest = line.strip_prefix("pub ").unwrap_or(line);
        let rest = rest.strip_prefix("effect ").unwrap_or(rest);
        let Some(rest) = rest.strip_prefix("fn ") else { continue };
        let Some(after) = rest.strip_prefix(func) else { continue };
        if !after.starts_with(['(', '[']) {
            continue;
        }
        let tail = &src[start + (line.len() - after.len())..];
        return params_of(tail).map(|ps| ps.iter().map(|p| is_fn_typed(p)).collect());
    }
    None
}

/// The top-level comma-separated parameters of `[generics](params) -> ...`.
fn params_of(tail: &str) -> Option<Vec<&str>> {
    let tail = match tail.strip_prefix('[') {
        Some(_) => &tail[closing(tail, '[', ']')? + 1..],
        None => tail,
    };
    if !tail.starts_with('(') {
        return None;
    }
    let inner = &tail[1..closing(tail, '(', ')')?];
    let mut out = Vec::new();
    let (mut depth, mut from) = (0i32, 0);
    for (i, c) in inner.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[from..]);
    Some(out.into_iter().filter(|p| !p.trim().is_empty()).collect())
}

/// The index of the bracket closing the one `s` opens with.
fn closing(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// `name: (..) -> ..` or `name: effect (..) -> ..` — a parenthesised type
/// followed by `->` (`(A?)?` alone is an Option, a tuple is no function).
fn is_fn_typed(param: &str) -> bool {
    let Some((_, ty)) = param.split_once(':') else { return false };
    let ty = ty.trim_start();
    let ty = ty.strip_prefix("effect ").map(str::trim_start).unwrap_or(ty);
    ty.starts_with('(')
        && closing(ty, '(', ')').is_some_and(|end| ty[end + 1..].trim_start().starts_with("->"))
}

#[cfg(test)]
mod tests {
    use super::callback_params;

    #[test]
    fn the_callback_is_the_fn_typed_position_and_a_type_variable_is_a_value() {
        assert_eq!(callback_params("list", "fold"), Some(vec![false, false, true]));
        assert_eq!(callback_params("list", "map"), Some(vec![false, true]));
        assert_eq!(callback_params("option", "unwrap_or"), Some(vec![false, false]));
        assert_eq!(callback_params("fs", "fold_lines"), Some(vec![false, false, true]));
        assert_eq!(callback_params("matrix", "map"), Some(vec![false, true]));
        assert_eq!(callback_params("list", "no_such_fn"), None);
    }
}
