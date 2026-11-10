// A generic Fn parameter already accepts the statically typed closure.
// Validate its complete callable bound; an opaque generic name is not an ABI.
// Keep the original runtime item and all its Clone/Send/Sync constraints.
fn generic_runtime_callback(d: &DTy, rust: &str, symbol: &str, items: &[&str]) -> bool {
    if !matches!(d, DTy::Fn(..)) || rust.is_empty()
        || !rust.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return false; }
    let Some(callback) = fan_rust_type(d) else { return false; };
    let prefix = format!("fn {symbol}<");
    let text = crate::native_floor::native_host_callback_body(symbol)
        .or_else(|| items.iter().find(|s| s.contains(&prefix)).copied());
    let Some(text) = text else { return false; };
    generic_callback_bound(text, &prefix, rust, &callback)
}

fn generic_callback_bound(text: &str, prefix: &str, rust: &str, callback: &str) -> bool {
    let Some(start) = text.find(prefix) else { return false; };
    let head = &text[start + prefix.len()..];
    let mut depth = 0i32;
    let mut from = 0;
    let mut bound = None;
    for (at, c) in head.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' if at > 0 && head.as_bytes()[at - 1] == b'-' => continue,
            '>' if depth == 0 => {
                let part = &head[from..at];
                if let Some((name, value)) = part.split_once(':') {
                    if name.trim() == rust {
                        if bound.is_some() { return false; }
                        bound = Some(value);
                    }
                }
                let Some(value) = bound else { return false; };
                let value: String = value.chars().filter(|c| !c.is_whitespace()).collect();
                return value == callback || value == format!("{callback}+Send+Sync");
            }
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                if let Some((name, value)) = head[from..at].split_once(':') {
                    if name.trim() == rust {
                        if bound.is_some() { return false; }
                        bound = Some(value);
                    }
                }
                from = at + 1;
            }
            '{' | ';' => return false,
            _ => {}
        }
        if depth < 0 { return false; }
    }
    false
}

#[cfg(test)]
mod generic_callback_tests {
    use super::*;
    #[test]
    fn generic_callback_checks_the_whole_function_contract() {
        let prefix = "fn walk<";
        let signature = "Fn(A,AlmideString)->A";
        for source in [
            "fn walk<A, F: Fn(A, AlmideString) -> A>(a: A, f: F) {}",
            "fn walk<A: Clone + Send, F: Fn(A, AlmideString) -> A + Send + Sync>(a: A, f: F) {}",
        ] { assert!(generic_callback_bound(source, prefix, "F", signature)); }
        for source in [
            "fn walk<A, F>(a: A, f: F) {}",
            "fn walk<A, F: FnMut(A, AlmideString) -> A>(a: A, f: F) {}",
            "fn walk<A, F: Fn(A, String) -> A>(a: A, f: F) {}",
            "fn walk<A, F: Fn(A, AlmideString) -> Result<A, String>>(a: A, f: F) {}",
            "fn walk<A, F: Fn(A, AlmideString) -> A + Unknown>(a: A, f: F) {}",
            "fn walk<A, F: Fn(A, AlmideString) -> A, F: Fn()>(a: A, f: F) {}",
        ] { assert!(!generic_callback_bound(source, prefix, "F", signature), "{source}"); }
    }
    #[test]
    fn fs_generic_routes_keep_original_runtime_functions() {
        for call in ["fs.fold_lines_range", "fs.fold_lines_chunked"] {
            let h = derive_explained(call).unwrap_or_else(|why| panic!("{call}: {why}"));
            assert_eq!(h.symbol, h.twin);
            assert!(h.params.iter().any(|(d, _)| matches!(d, DTy::Fn(..))));
        }
    }
}
