//! `almide-lin-prove <artifact.wasm> <anchors>` — prints the linear
//! certificate (`;`-separated `Q` stretches) for the module's self-host
//! bodies on stdout, and each body left out, with why, on stderr.
//!
//! `almide-lin-prove --routines <artifact.wasm> <anchors> <selection-out>`
//! — the same for the runtime routines the anchor file's `routines` line
//! names (#4005 A4): the certificate on stdout, its site selection written
//! to `<selection-out>`, and per routine on stderr the accesses certified
//! and left counted (or why it was left out).

use std::io::Write;

fn read_inputs(wasm: &str, anchors: &str) -> (Vec<u8>, String) {
    let w = std::fs::read(wasm).unwrap_or_else(|e| {
        let _ = writeln!(std::io::stderr(), "{wasm}: {e}");
        std::process::exit(2)
    });
    let a = std::fs::read_to_string(anchors).unwrap_or_else(|e| {
        let _ = writeln!(std::io::stderr(), "{anchors}: {e}");
        std::process::exit(2)
    });
    (w, a)
}

fn routines(wasm: &str, anchors: &str, sel_out: &str) {
    let (w, a) = read_inputs(wasm, anchors);
    let names = almide_anchor_check::lin::routine_names(&a);
    let name = |p: usize| names.get(&p).cloned().unwrap_or_else(|| "?".into());
    match almide_lin_prove::certify_routines(&w, &a) {
        Ok((cert, sel, refused, counts)) => {
            if let Err(e) = std::fs::write(sel_out, format!("{sel}\n")) {
                let _ = writeln!(std::io::stderr(), "LIN PROVE FAIL: {sel_out}: {e}");
                std::process::exit(1);
            }
            println!("{cert}");
            for (p, (certified, counted)) in counts {
                let _ = writeln!(std::io::stderr(), "routine\t{p}\t{}\t{certified}\t{counted}", name(p));
            }
            for (p, why) in refused {
                let _ = writeln!(std::io::stderr(), "unrouted\t{p}\t{}\t{why}", name(p));
            }
            let (budget, used) = almide_lin_prove::effort();
            let _ = writeln!(std::io::stderr(), "effort\t{used}\tof\t{budget}\tpivots");
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "LIN PROVE FAIL: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, wasm, anchors, sel_out] = args.as_slice() {
        if flag == "--routines" {
            routines(wasm, anchors, sel_out);
            return;
        }
    }
    let [wasm, anchors] = args.as_slice() else {
        let _ = writeln!(
            std::io::stderr(),
            "usage: almide-lin-prove <artifact.wasm> <anchors>\n       almide-lin-prove --routines <artifact.wasm> <anchors> <selection-out>"
        );
        std::process::exit(2);
    };
    let (w, a) = read_inputs(wasm, anchors);
    let names = almide_anchor_check::lin::selfhost_names(&a);
    let sites = almide_anchor_check::lin::prim_sites(&a).unwrap_or_default();
    let name = |p: usize| names.get(&p).cloned().unwrap_or_else(|| "?".into());
    let n = |p: usize| sites.get(&p).map_or(0, std::collections::BTreeSet::len);
    match almide_lin_prove::certify_table(&w, &a) {
        Ok((cert, refused, table)) => {
            println!("{cert}");
            for p in table {
                let _ = writeln!(std::io::stderr(), "certified\t{p}\t{}\t{}", name(p), n(p));
            }
            for (p, why) in refused {
                let _ = writeln!(std::io::stderr(), "uncertified\t{p}\t{}\t{}\t{why}", name(p), n(p));
            }
            let (budget, used) = almide_lin_prove::effort();
            let _ = writeln!(std::io::stderr(), "effort\t{used}\tof\t{budget}\tpivots");
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "LIN PROVE FAIL: {e}");
            std::process::exit(1);
        }
    }
}
