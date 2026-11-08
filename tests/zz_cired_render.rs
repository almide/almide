#[test]
fn cired_render() {
    for p in std::env::var("CIRED_SRCS").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
        let src = std::fs::read_to_string(p).expect("src");
        match almide_mir::pipeline::try_render_rust_source(&src) {
            Ok(r) => println!("=== RENDER {p}\n{r}"),
            Err(e) => println!("=== RENDER {p} WALL {e:?}"),
        }
    }
}
