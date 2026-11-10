//! #4631: a top-level loop opens `(` on each loop-carried slot's stream and
//! must close exactly those streams. A slot rebound to another object inside
//! the body (`acc = c` after `var c = acc` in a `while`) used to close the
//! NEW object's stream: a lone `(` on the first line and a stray `)` on the
//! second, which only the old lenient parser repaired. With the parser now
//! poisoning both shapes, every line of these frames must be well delimited.

fn delimited(line: &str) -> bool {
    let mut depth = 0i32;
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if !(0..=1).contains(&depth) {
            return false;
        }
    }
    depth == 0
}

fn frames(fixture: &str, names: &[&str]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join(fixture)).expect("read the fixture");
    let certs = almide_mir::pipeline::ownership_certificates(&source).expect("the fixture must lower");
    for name in names {
        let (_, cert) = certs.iter().find(|(n, _)| n.ends_with(name)).unwrap_or_else(|| panic!("no frame {name} in {fixture}"));
        for line in cert.lines() {
            assert!(delimited(line), "{fixture}::{name}: line {line:?} is not well delimited in {cert:?}");
        }
    }
}

#[test]
fn a_slot_rebound_inside_its_loop_closes_the_stream_the_loop_opened() {
    frames("spec/wasm_cross/mut_param_copy_kept_when_read.almd", &["read_in_later_pass"]);
    frames("spec/wasm_cross/mut_param_copy_moved_when_dead.almd", &["rebound_in_loop"]);
}

#[test]
fn the_delimiter_check_refuses_the_old_shapes() {
    // The two lines the recorder wrote before the fix.
    assert!(!delimited("("));
    assert!(!delimited("i(abbadbadd))d"));
    assert!(delimited("i(abbadbadd)d"));
}
