//! Probe: render a source natively OR dump the MIR ops (--mir).
fn main() {
    let path = std::env::args().nth(1).expect("usage: probe_native <file.almd> [--mir]");
    let src = std::fs::read_to_string(&path).expect("failed to read the source file");
    if std::env::args().nth(2).as_deref() == Some("--mir") {
        match almide_mir::pipeline::debug_dump_mir(&src) {
            Ok(dump) => println!("{dump}"),
            Err(e) => println!("IR-WALL: {e:?}"),
        }
        return;
    }
    match almide_mir::pipeline::try_render_rust_source(&src) {
        Ok(code) => println!("RENDERED:\n{code}"),
        Err(e) => println!("WALL: {e:?}"),
    }
}
