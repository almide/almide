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
    match almide_lin_prove::certify(&w, &a) {
        Ok((cert, refused)) => {
            println!("{cert}");
            for (p, why) in refused {
                let _ = writeln!(std::io::stderr(), "uncertified {p}: {why}");
            }
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "LIN PROVE FAIL: {e}");
            std::process::exit(1);
        }
    }
}
