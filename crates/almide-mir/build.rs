// Run the same conservative scanner used by the verification tests once at
// compiler build time, rather than once per user compilation.
#![allow(dead_code)]
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};
use std::fmt::Write;

#[path = "src/render_native_str.rs"]
mod render_native_str;
include!("src/native_floor_header.rs");
include!("src/native_floor_index.rs");
include!("src/native_floor_build.rs");
include!("src/native_floor_scan.rs");
include!("src/native_floor_host.rs");

fn main() {
    // Cargo also tracks the codegen build dependency and its embedded runtime.
    for file in ["build.rs", "src/render_native_str.rs", "src/native_floor_header.rs", "src/native_io_header.rs",
        "src/native_floor_index.rs", "src/native_floor_build.rs", "src/native_floor_scan.rs", "src/native_floor_host.rs", "src/native_floor_cfg.rs"] {
        println!("cargo:rerun-if-changed={file}");
    }
    let index = build_runtime_index();
    let mut out = String::from("{\nlet mut items = HashMap::new();\nlet mut fs = BTreeSet::new();\n");
    let sorted: BTreeMap<_, _> = index.items.iter().collect();
    for (name, item) in sorted {
        // Preserve module slices: fs provenance and callers' slice identity
        // checks must still refer to the embedded source after deserialization.
        let slice = almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES.iter()
            .enumerate().find_map(|(i, (_, src))| {
                let start = item.text.as_ptr() as usize;
                let base = src.as_ptr() as usize;
                (start >= base && start + item.text.len() <= base + src.len())
                    .then(|| (i, start - base, start - base + item.text.len()))
            });
        if let Some((i, start, end)) = slice {
            writeln!(out, "let text = &almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES[{i}].1[{start}..{end}];").expect("writing to a String cannot fail");
        } else {
            writeln!(out, "let text: &'static str = {:?};", item.text).expect("writing to a String cannot fail");
        }
        if is_fs_host_text_in(&index, item.text) {
            out.push_str("fs.insert(text.as_ptr() as usize);\n");
        }
        writeln!(out, "items.insert({name:?}, RtItem {{ text }});").expect("writing to a String cannot fail");
    }
    out.push_str("RuntimeIndex { items, fs_built: fs,\n");
    for (name, set) in [("defined", index.defined), ("trait_methods", index.trait_methods), ("fns", index.fns)] {
        writeln!(out, "{name}: {:?}.into_iter().collect(),", set.into_iter().collect::<Vec<_>>()).expect("writing to a String cannot fail");
    }
    out.push_str("}\n}\n");
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts")).join("native_floor_index.rs");
    std::fs::write(path, out).expect("OUT_DIR is writable");
}
