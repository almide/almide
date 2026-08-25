//! #1574: a user type named with a generic-parameter LETTER another module
//! happens to use (`T`, or any of the stdlib's A B C E F K T U V) must be an
//! ordinary name. Registration and the checker used to install a signature's
//! generic letters into `env.types` and then REMOVE them — destroying, not
//! shadowing, a same-named user type's bare binding — so a later-registered
//! module's generics silently unresolved the type and bare-`self` protocol
//! satisfaction failed cross-module ("parameter 'self' has type 'T', expected
//! '{ n: Int }'"). The reserved letter set was invisible and unstable (it
//! depended on which letters any linked module used). Now every generic-letter
//! scope shadows and RESTORES.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

/// The issue's repro, verbatim shape: protocol in ONE module, a bare-`self`
/// convention method on a type named `T` in ANOTHER, satisfaction consumed
/// through a generic bound in main — plus the user-generic variant (a type
/// named `Q` while main's own fn uses generic letters), which joined the
/// reserved set the same way. The consuming bound is spelled `S` so the type
/// name never equals the consumer's OWN generic letter — that local collision
/// (`go[Q]` applied AT a type named `Q`) is a separate mono-level bug, filed
/// on its own; this pin covers the cross-module reserved-set class.
#[test]
fn generic_letter_type_names_are_ordinary_names() {
    let dir = std::env::temp_dir().join(format!("almd_genletter_{}", std::process::id()));
    for type_name in ["T", "Q"] {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("almide.toml"),
            "[package]\nname = \"genletter\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/ports.almd"),
            "protocol Proto {\n  fn read(self) -> Int\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/infra.almd"),
            format!(
                "import self.ports\n\ntype {tn}: Proto = {{ n: Int }}\n\n\
                 fn {tn}.read(self) -> Int = self.n\n",
                tn = type_name
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.almd"),
            format!(
                "import self.ports\nimport self.infra\nimport io\n\n\
                 fn go[S: Proto](c: S) -> Int = c.read()\n\n\
                 effect fn main() -> Unit = {{ io.print(int.to_string(go(infra.{tn} {{ n: 7 }}))) }}\n",
                tn = type_name
            ),
        )
        .unwrap();

        for target in ["--target=wasm", "--target=rust"] {
            let out = Command::new(almide_bin())
                .args(["run", dir.join("src/main.almd").to_str().unwrap(), target])
                .output()
                .expect("failed to spawn almide run");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                stdout.contains('7'),
                "type '{type_name}' must be an ordinary name on {target}: stdout={stdout} stderr={}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}
