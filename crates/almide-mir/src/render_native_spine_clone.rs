//! The copy of a recursive variant type follows its chain in a loop.
//!
//! A derived `Clone` copies a boxed field by calling itself: one native
//! stack frame per level. A value that is a long chain of one case (`LSucc`
//! of `LSucc` of …, a cons list, a left- or right-leaning tree) then walks
//! megabytes of stack per copy, and a function taking such a value by value
//! copies it on every call — 20,000 levels per call cost a quarter more than
//! the standard codegen's queue-driven copy.
//!
//! Here the copy descends the LAST field of each case that is the type
//! itself, boxed, without a call: the parent is written first with a
//! placeholder in that field, and the loop moves into the placeholder. Every
//! other field is copied by its own `clone` as before, so the result is the
//! value a derived `Clone` builds, field for field.
//!
//! ```text
//! enum T { C0, C1(Box<T>), C2(Box<T>, i64, Box<T>), C3(AlmideString) }
//!
//! Self::C2(f0, f1, f2) => {
//!     std::mem::forget(std::mem::replace(dst, Self::C2(f0.clone(), f1.clone(), Box::new(Self::C0))));
//!     dst = match dst { Self::C2(_, _, d) => &mut **d, _ => unreachable!() };
//!     src = &**f2;
//! }
//! ```
//!
//! The placeholder is a case without fields: it owns nothing, so replacing
//! it needs no release (`forget`). A type with no such case, or with no case
//! whose last field is the type itself, keeps the derived copy.

/// One case of the type: its tag, and per field whether it is the type
/// itself in a box.
pub(crate) struct Case {
    pub(crate) tag: u32,
    pub(crate) own_box: Vec<bool>,
}

/// `impl Clone` for the enum `name`, or `None` when the derived one stays.
pub(crate) fn clone_impl(name: &str, cases: &[Case]) -> Option<String> {
    let hole = cases.iter().find(|c| c.own_box.is_empty())?.tag;
    if !cases.iter().any(|c| c.own_box.last() == Some(&true)) {
        return None;
    }
    let mut arms = String::new();
    for c in cases {
        let t = c.tag;
        let n = c.own_box.len();
        if n == 0 {
            arms.push_str(&format!("                Self::C{t} => {{\n                    std::mem::forget(std::mem::replace(dst, Self::C{t}));\n                    break;\n                }}\n"));
            continue;
        }
        let binds = (0..n).map(|k| format!("f{k}")).collect::<Vec<_>>().join(", ");
        let descends = c.own_box[n - 1];
        let copies = (0..n)
            .map(|k| if descends && k == n - 1 { format!("Box::new(Self::C{hole})") } else { format!("f{k}.clone()") })
            .collect::<Vec<_>>()
            .join(", ");
        arms.push_str(&format!("                Self::C{t}({binds}) => {{\n                    std::mem::forget(std::mem::replace(dst, Self::C{t}({copies})));\n"));
        if descends {
            let skip = "_, ".repeat(n - 1);
            arms.push_str(&format!("                    dst = match dst {{ Self::C{t}({skip}d) => &mut **d, _ => unreachable!() }};\n                    src = &**f{};\n                }}\n", n - 1));
        } else {
            arms.push_str("                    break;\n                }\n");
        }
    }
    Some(format!(
        "impl Clone for {name} {{\n    fn clone(&self) -> Self {{\n        let mut root = Self::C{hole};\n        let mut src = self;\n        let mut dst = &mut root;\n        loop {{\n            match src {{\n{arms}            }}\n        }}\n        root\n    }}\n}}\n"
    ))
}
