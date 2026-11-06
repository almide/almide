//! RoutineLinCert.v's examples and RoutineLin.v's controls through the byte
//! format (lin.rs's tests).
use crate::bounds::check_bounds_cert;

fn slot_text(c: &str, w: &str) -> String {
    format!(
        "Q3{{ | | 0 | h0,7 f7 n1,7,4 T+ []{c} [s1:1]0 ()1()1 ()1()1 ()1()1 {{ R{{}} | A7,{w},12 [s0:1]0 (7:1 3:1)1 (7:1)1 (6:1 0:4 2:1)1 R{{}} }} }}"
    )
}

// The Examples of proofs/RoutineLinCert.v, verdict for verdict.
#[test]
fn the_coq_examples() {
    assert!(check_bounds_cert(b"c0 a0 c1 a1 w0 w1"));
    assert!(!check_bounds_cert(b"c0 s0 a0"));
    assert!(check_bounds_cert(slot_text("1", "8").as_bytes()));
    assert!(!check_bounds_cert(slot_text("0", "8").as_bytes()));
    assert!(!check_bounds_cert(b"Q3{ | | 0 | h0,7 }"));
    assert!(!check_bounds_cert(b"Q3{ | | 0 | R{} f7 }"));
    assert!(!check_bounds_cert(b"Q3{ | | 0 | R{} } ; Q3{ | | 0 | R{} }"));
    assert!(!check_bounds_cert(b"c0 s0 a0 ; Q3{ | | 0 | R{} }"));
    assert!(!check_bounds_cert(b"Q3{ | | 0 | h0,7 f7 A7,4,4 [s0:1]0 (4:1 0:1)1 (4:1)1 (3:1 1:-1)1 R{} }"));
    assert!(check_bounds_cert(b"Q3{ | | 0 | h0,7 f7 A7,4,4 [s0:1]0 (4:1 0:1)1 (4:1)1 (3:1 1:1)1 R{} }"));
    // An allocation of 8 bytes (its size proven in [0, 2^32) by the
    // constant itself), then a read of its first 8 payload bytes:
    // facts after `f4`: 0 `-b4`, 1 `-c4`, 2 wf, 3 `s0 - b4`, 4 `b4 - s0`,
    // 5 `8 - c4`.
    let al = |n: &str| format!("Q1{{ | | 0 | a0,4 []8 ()1 ()1 f4 A4,{n},12 [s0:1]0 (4:1 0:1)1 (4:1)1 (3:1 5:1)1 R{{}} }}");
    assert!(check_bounds_cert(al("8").as_bytes()));
    assert!(!check_bounds_cert(al("9").as_bytes()));
    let st = |off: &str| format!("Q1{{ | | 0 | g2,144,18756 A2,1,{off} []0 ()1 (0:1)1 (1:1 3:1)1 R{{}} }}");
    assert!(check_bounds_cert(st("256").as_bytes()));
    assert!(!check_bounds_cert(st("18912").as_bytes()));
}

// RoutineLin.v's controls, as bytes: the same programs inside a
// procedure that returns.
#[test]
fn the_routine_lin_controls() {
    let hdr = |off: &str, p2: &str| format!("Q0{{ | | 0 | h0,7 f7 A7,4,{off} [s0:1]0 (4:1 0:1)1 (4:1)1 {p2} R{{}} }}");
    assert!(check_bounds_cert(hdr("4", "(3:1 1:1)1").as_bytes()));
    assert!(!check_bounds_cert(hdr("12", "(3:1 1:1)1").as_bytes()));
    assert!(!check_bounds_cert(hdr("4", "(3:1 1:-1)1").as_bytes()));
    assert!(check_bounds_cert(slot_text("0", "4").as_bytes()));
    assert!(!check_bounds_cert(b"Q0{ | | 0 | c{} }"));
    assert!(!check_bounds_cert(b"Q0{ | | 0 | C1{ | | | 0 | } R{} }"));
    assert!(!check_bounds_cert(b"Q0{ | | 0 | }"));
    assert!(!check_bounds_cert(b"Q0{ | []1 | 0 | R{ ()1 } }"));
}

// A recursive procedure: slot 0 a pointer walking region 7's payload to
// the end pointer slot 1, reading a byte at each step, then calling
// itself with 0 := s0 + 1. Pre: 0 `b7 - s0 + 12` (VB7 + 12 <= s0),
// 1 `s1 - b7 - c7 - 12` (s1 <= VB7 + 12 + VC7). After `f7` the facts are
// 0 `-b7`, 1 `-c7`, 2 wf, 3 and 4 the pre; the test's else arm puts
// `s0 - s1 + 1` first (indices shift by one). The read's proofs:
// `-s0` = 4 + 1 (const 12); `b7 - s0` = 4 (const 12); `s0 - b7 - c7 - 11`
// = 0 + 5 exactly. The argument s0 + 1's no-wrap: low from 4 + 1, high
// from 0 + 5 + 3. The translated pre: `b7 - s0 + 11` from 4, and P1
// verbatim.
#[test]
fn a_recursive_walk() {
    let pre = "[b7:1 s0:-1]12 [s1:1 b7:-1 c7:-1]-12";
    let walk = format!(
        "Q5{{ {pre} | | 0 | f7 T+ [s0:1]0 [s1:1]0 ()1()1 ()1()1 ()1()1 {{ R{{}} | \
         A7,1,0 [s0:1]0 (4:1 1:1)1 (4:1)1 (0:1 5:1)1 \
         C5{{ 0:[s0:1]1 ! (4:1 1:1)1 (0:1 5:1 3:1)1 1:[s1:1]0 ! ()1 ()1 | 7:7 | | 9 | (4:1)1 (5:1)1 }} R{{}} }} }}"
    );
    assert!(check_bounds_cert(walk.as_bytes()), "{walk}");
    // The same walk reading 2 bytes: refused (the test proves one byte).
    let bad = walk.replace("A7,1,0", "A7,2,0");
    assert!(!check_bounds_cert(bad.as_bytes()));
}
