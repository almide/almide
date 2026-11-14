//! Mutable scalar locals are declared in source order.
//!
//! A straight-line reassignment of a `var` (`var a = 0.0 … a = f(k)`) lowers
//! to a new value, so its local would be declared at the reassignment, after
//! every `var` declared in between. A backend lays out a loop's carried
//! values in declaration order, and which neighbours it packs together
//! follows from that order. The lowering records the reassigned value's
//! origin ([`crate::MirFunction::slot_origins`]); here the local's
//! declaration moves up to where its `var` was declared and the reassignment
//! becomes a plain assignment to it:
//!
//! ```text
//! let mut v2: i64 = 2i64;            let mut v2: i64 = 2i64;
//! let mut v3: f64 = …;               let mut v8: i64;
//! let mut v8: i64 = v2 + v7;   ⇒     let mut v3: f64 = …;
//!                                    v8 = v2 + v7;
//! ```
//!
//! Only a local declared once, on one line, at the function body's own level
//! moves, and only to another such line of that level: the declaration and
//! its assignment stay in one block, so the local is assigned before every
//! read exactly as before. Anything else is left as rendered.

use crate::render_native::var;
use crate::{MirFunction, ValueId};

/// The body level's indentation (`render_fn` starts at one level).
const PAD: &str = "    ";

/// `let mut v<N>: T = …` at the body level → `N`. A declaration this pass
/// moved up has no value on its line, so it is never taken for a place.
fn declared(line: &str) -> Option<u32> {
    let rest = line.strip_prefix(PAD)?.strip_prefix("let mut v")?;
    let (digits, rest) = rest.split_once(": ")?;
    rest.contains(" = ").then(|| digits.parse().ok())?
}

/// A one-line scalar declaration of `v` at the body level → (type, value).
fn scalar_decl(line: &str, v: ValueId) -> Option<(&'static str, &str)> {
    let rest = line.strip_prefix(PAD)?.strip_prefix("let mut ")?.strip_prefix(var(v).as_str())?;
    let (ty, rest) = [("f64", ": f64 = "), ("i64", ": i64 = ")].into_iter().find_map(|(ty, head)| Some((ty, rest.strip_prefix(head)?)))?;
    Some((ty, rest.strip_suffix(';')?))
}

/// Move each recorded reassignment's declaration up to its `var`'s place.
pub(crate) fn declare_in_source_order(func: &MirFunction, out: &mut String) {
    if func.slot_origins.is_empty() {
        return;
    }
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut moved = false;
    for (&new, &origin) in &func.slot_origins {
        let head = format!("let mut {}:", var(new));
        let mut sites = lines.iter().enumerate().filter(|(_, l)| l.trim_start().starts_with(&head));
        let (Some((at, line)), None) = (sites.next(), sites.next()) else { continue };
        let Some((ty, value)) = scalar_decl(line, new) else { continue };
        // The first body-level declaration made after the `var`'s own.
        let Some(to) = lines[..at].iter().position(|l| declared(l).is_some_and(|n| n > origin.0)) else { continue };
        lines[at] = format!("{PAD}{} = {value};", var(new));
        lines.insert(to, format!("{PAD}let mut {}: {ty};", var(new)));
        moved = true;
    }
    if moved {
        *out = lines.join("\n");
        out.push('\n');
    }
}


#[cfg(test)]
#[path = "native_slot_order_test.rs"]
mod tests;
