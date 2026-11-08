#[test]
fn cired_render() {
    let dir = std::env::var("CIRED_PROGS").unwrap_or_else(|_| ".github/cired/progs".into());
    let mut fs: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    fs.sort();
    for f in fs {
        let src = std::fs::read_to_string(&f).unwrap();
        match almide_mir::pipeline::try_render_rust_source(&src) {
            Ok(r) => { let at = r.find("__ALMIDE_V1_RT_END__").map(|i| i + 22).unwrap_or(0); println!("=== RENDER {}\n{}", f.display(), &r[at..]); }
            Err(e) => println!("=== RENDER {} WALL {e:?}", f.display()),
        }
    }
}
