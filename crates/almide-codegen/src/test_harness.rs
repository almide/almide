//! Native libtest framing shared by the Rust generators. Test bodies and
//! their witnesses belong to the caller; names, completion records and the
//! ignored abort-site entry must follow the same harness protocol.

use crate::template::TemplateSet;

/// A module that the native builder can move into a separate source file.
pub fn module_file_group(name: &str, body: &str) -> String {
    use almide_base::names::{MODULE_FILE_BEGIN, MODULE_FILE_END};
    format!("{MODULE_FILE_BEGIN}{name}\nmod {name} {{\nuse super::*;\n\n{body}\n}}\n{MODULE_FILE_END}{name}")
}

/// Frame already-rendered test items, retaining first-seen module order.
/// `origin` is the same canonical module identifier used for function names.
pub fn render_module(templates: &TemplateSet, items: Vec<(Option<String>, String)>) -> String {
    let mut parts = Vec::new();
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for (origin, rendered) in items {
        match origin {
            Some(origin) => {
                let name = almide_base::names::tests_module(&origin);
                match grouped.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, tests)) => tests.push(rendered),
                    None => grouped.push((name, vec![rendered])),
                }
            }
            None => parts.push(rendered),
        }
    }
    for (name, tests) in grouped {
        parts.push(module_file_group(&name, &tests.join("\n\n")));
    }
    let tests = parts.join("\n\n");
    let indented = crate::walker::helpers::indent_lines(&tests, 4);
    templates.render_with("test_module", None, &[], &[("tests", indented.as_str())])
        .unwrap_or(tests)
}

/// The ignored child entry used by `testing.assert_aborts`. The dispatcher
/// is a compiled Almide function; the wrapper only implements libtest's ABI.
pub fn abort_entry(templates: &TemplateSet, dispatch: &str, entry: &str) -> Option<String> {
    abort_entry_params(templates, dispatch, entry, "", "almide_abort", "almide_stdout_flush", false)
}

/// The same child ABI with the renderer's initialization and runtime helpers.
pub fn abort_entry_with(templates: &TemplateSet, dispatch: &str, entry: &str, init: &str, abort: &str, flush: &str) -> Option<String> {
    abort_entry_params(templates, dispatch, entry, init, abort, flush, true)
}

fn abort_entry_params(templates: &TemplateSet, dispatch: &str, entry: &str, init: &str, abort: &str, flush: &str, any_ok: bool) -> Option<String> {
    templates.render_with("abort_site_entry", None, &[], &[
        ("dispatch", dispatch), ("entry", entry), ("init", init), ("abort", abort), ("flush", flush),
        ("outcome_params", if any_ok { "T, E: std::fmt::Display" } else { "E: std::fmt::Display" }),
        ("outcome_ok", if any_ok { "T" } else { "()" }),
    ])
}
