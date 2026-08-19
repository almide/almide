// TEMP debug — DELETE. Dumps the WAT for the m7 repro.
#[test]
fn zz_wat_dump() {
    let src = std::fs::read_to_string("/tmp/i1537/m7.almd").unwrap();
    let wat = almide_mir::pipeline::try_render_wasm_source(&src, &[], true)
        .expect("render");
    std::fs::write("/tmp/i1537/m7.wat", &wat).unwrap();
    eprintln!("WAT {} bytes", wat.len());
}
