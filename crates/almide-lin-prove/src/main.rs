//! `almide-lin-prove <artifact.wasm> <anchors>` — prints the linear
//! certificate (`;`-separated `Q` stretches) for the module's self-host
//! bodies on stdout, and each body left out, with why, on stderr.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [wasm, anchors] = args.as_slice() else {
        let _ = writeln!(std::io::stderr(), "usage: almide-lin-prove <artifact.wasm> <anchors>");
        std::process::exit(2);
    };
    let w = std::fs::read(wasm).unwrap_or_else(|e| {
        let _ = writeln!(std::io::stderr(), "{wasm}: {e}");
        std::process::exit(2)
    });
    let a = std::fs::read_to_string(anchors).unwrap_or_else(|e| {
        let _ = writeln!(std::io::stderr(), "{anchors}: {e}");
        std::process::exit(2)
    });
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
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "LIN PROVE FAIL: {e}");
            std::process::exit(1);
        }
    }
}
