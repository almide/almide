//! Extract the fan gate protocol from the runtime source and point it at loom.
//!
//! The span is `fn almide_fan_deal` .. the end of `fn almide_fan_steal` in
//! runtime/rs/src/list.rs: the offer (publish, take, withdraw, wait), the gate
//! and its `unsafe impl Sync`, the helper's entry and exit, the slot table.
//! The pool's resident loop and its wake-ups lie outside it: they reach a gate
//! only through `almide_fan_steal` and `almide_fan_help`, which the model's
//! helper threads call directly.
//!
//! The rewrite is by syntax, never by item name:
//! - `std::sync::atomic::`, `std::sync::Mutex`, `std::cell::UnsafeCell`,
//!   `std::thread::`, `std::time::Instant` become the shim's (src/shim.rs);
//!   `std::thread_local!` and `std::hint::spin_loop` become loom's.
//! - a `thread_local!` `const { E }` initialiser becomes `E` (loom's macro
//!   has no const form);
//! - a top-level `const` whose initialiser builds a shim type (loom's
//!   primitives are not const-constructible) becomes a `fn`, and an array
//!   repeat of it becomes `std::array::from_fn`;
//! - a top-level `static` becomes a `loom::lazy_static!` (loom resets those
//!   per execution; a plain static would carry one execution's objects into
//!   the next).
//!
//! `LOOM_FAN_CONTROL` breaks the protocol on purpose (the negative controls
//! of scripts/loom-fan-gate.sh); each control's anchor must occur exactly
//! once, or the build fails rather than run an unbroken model as a control.

use std::fs;
use std::path::PathBuf;

const SOURCE: &str = "../../runtime/rs/src/list.rs";
const FIRST: &str = "fn almide_fan_deal<";
const LAST: &str = "fn almide_fan_steal(";

/// (control, anchor, replacement). `nowait`: the caller returns without
/// waiting for the helpers inside its gate. `noreaders`: withdrawing an offer
/// does not wait for the threads that are reading its slot.
const CONTROLS: &[(&str, &str, &str)] = &[
    ("nowait", "if gate.inside.load(Ordering::SeqCst) == 0 {", "if true {"),
    ("noreaders", "while slot.readers.load(Ordering::SeqCst) > 0 {", "while false {"),
];

fn main() {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-env-changed=LOOM_FAN_CONTROL");
    let src = fs::read_to_string(SOURCE).expect("read runtime/rs/src/list.rs");
    let lines: Vec<&str> = src.lines().collect();
    let start = lines.iter().position(|l| l.starts_with(FIRST)).expect("`fn almide_fan_deal` is gone from list.rs");
    let last = lines.iter().position(|l| l.starts_with(LAST)).expect("`fn almide_fan_steal` is gone from list.rs");
    let end = last + lines[last..].iter().position(|l| *l == "}").expect("the end of `fn almide_fan_steal`");
    let span = &lines[start..=end];

    let mut out = String::new();
    let mut fn_consts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < span.len() {
        let line = span[i];
        if line.starts_with("const ") {
            // gather the whole item (it may span lines)
            let mut item = String::from(line);
            while !item.trim_end().ends_with(';') {
                i += 1;
                item.push('\n');
                item.push_str(span[i]);
            }
            let item = rewrite_paths(&item);
            if item.contains("crate::shim::") {
                let (head, init) = item.split_once(" = ").expect("const NAME: T = E;");
                let (name, ty) = head["const ".len()..].split_once(": ").expect("const NAME: T");
                let init = init.trim_end().trim_end_matches(';');
                out.push_str(&format!("fn {name}() -> {ty} {{ {init} }}\n"));
                fn_consts.push(name.to_string());
            } else {
                out.push_str(&item);
                out.push('\n');
            }
            i += 1;
            continue;
        }
        let mut l = rewrite_paths(line);
        if l.contains("= const { ") && l.trim_end().ends_with(" };") {
            l = l.replacen("= const { ", "= ", 1);
            let cut = l.rfind(" };").unwrap();
            l = format!("{};", &l[..cut]);
        }
        if l.starts_with("static ") {
            l = format!("loom::lazy_static! {{ static ref {} }}", &l["static ".len()..]);
        }
        out.push_str(&l);
        out.push('\n');
        i += 1;
    }
    for name in &fn_consts {
        // `[NAME; N]` -> `std::array::from_fn(|_| NAME())`
        let pat = format!("[{name}; ");
        while let Some(at) = out.find(&pat) {
            let close = at + out[at..].find(']').unwrap();
            out.replace_range(at..=close, &format!("std::array::from_fn(|_| {name}())"));
        }
    }

    let control = std::env::var("LOOM_FAN_CONTROL").unwrap_or_default();
    if !control.is_empty() {
        let (_, anchor, with) = CONTROLS.iter().find(|(c, ..)| *c == control).unwrap_or_else(|| panic!("unknown LOOM_FAN_CONTROL {control:?}"));
        let n = out.matches(anchor).count();
        assert_eq!(n, 1, "control {control}: its anchor `{anchor}` occurs {n} times in the gate source — update the control");
        out = out.replacen(anchor, with, 1);
    }

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("gate.rs");
    fs::write(dest, out).unwrap();
}

fn rewrite_paths(s: &str) -> String {
    s.replace("std::sync::atomic::", "crate::shim::atomic::")
        .replace("std::sync::Mutex", "crate::shim::Mutex")
        .replace("std::cell::UnsafeCell", "crate::shim::UnsafeCell")
        .replace("std::thread_local!", "loom::thread_local!")
        .replace("std::thread::", "crate::shim::thread::")
        .replace("std::time::Instant", "crate::shim::Instant")
        .replace("std::hint::spin_loop", "loom::hint::spin_loop")
}
