//! Print the NATIVE leg's certificate bundle for one program (#4006): the
//! ownership witness of every function the verified native renderer
//! (`almide_mir::pipeline::try_render_rust_source_certified`) rendered,
//! taken from the exact MIR it rendered, in the format `almide-verify
//! bundle` reads.
//!
//! Usage: emit_native_bundle <program.almd> <out.rs>
//!
//! The rendered Rust source is written to `<out.rs>`, and the bundle (version
//! 2) pins its SHA-256: almide-verify recomputes the digest from the file and
//! rejects a mismatch, so the verdict is about THAT source. Exit 2 when the
//! program walls (the native build falls back to the standard codegen, which
//! carries no certificate) or a file cannot be written.
//!
//! What this certifies is the MIR the source was rendered from: RC balance
//! over its Inc/Dec events. That rustc compiles the rendered source into a
//! binary that keeps the balance — `Dup` as `.clone()`, `Drop` erased to
//! Rust's scope-end drop — is trusted.

use std::io::Write;

fn fail(msg: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{msg}");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [src, out] = args.as_slice() else { fail("usage: emit_native_bundle <program.almd> <out.rs>") };
    let text = std::fs::read_to_string(src).unwrap_or_else(|e| fail(&format!("read {src}: {e}")));
    let (rust, certs) = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source_certified(&text))
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e))
        .unwrap_or_else(|e| fail(&format!("{src} walls on the verified native renderer: {e}")));
    std::fs::write(out, &rust).unwrap_or_else(|e| fail(&format!("write {out}: {e}")));
    use sha2::Digest;
    let hex: String = sha2::Sha256::digest(rust.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let path = std::path::absolute(out).map(|p| p.display().to_string()).unwrap_or(out.clone());
    let mut bundle = format!(
        "almide-certificate-bundle 2\nproducer almide-mir emit_native_bundle\nsource {src}\nartifact sha256 {hex} {path}\n"
    );
    for (name, cert) in certs {
        bundle.push_str(&format!("witness ownership {} {name}\n{cert}\n", cert.len()));
    }
    let _ = std::io::stdout().write_all(bundle.as_bytes());
}
