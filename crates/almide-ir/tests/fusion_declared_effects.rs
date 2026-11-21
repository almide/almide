//! purity.fusion-effect-modules (#4855): stream fusion's "is this stdlib
//! module pure" is judged from the module's declarations. A fresh module that
//! no list names and that declares an `effect fn` (or an `@ordered` fn) is not
//! pure; one whose source cannot be read is not pure; one that declares only
//! plain fns is (the control).
#[path = "../../../tests/support/novel.rs"]
mod novel;

use almide_ir::fusion::module_is_pure_by_declaration;
use almide_lang::stdlib_info::source_declares_effect;

#[test]
fn novel_a_module_declaring_an_effect_is_not_fused_as_pure() {
    let module = novel::module();
    let f = novel::ident("touch");
    let effect_src = format!("/// a fresh module\neffect fn {f}(path: String) -> Result[Unit, String] = ok(())\n");
    let ordered_src = format!("@ordered\nfn {f}(n: Int) -> Int = n\n");
    let plain_src = format!("fn {f}(n: Int) -> Int = n + 1\n");
    for (what, src) in [("effect fn", &effect_src), ("@ordered fn", &ordered_src)] {
        let declared = source_declares_effect(src);
        assert!(
            !module_is_pure_by_declaration(&module, |_| true, |_| Some(declared)),
            "a stdlib module `{module}` declaring an {what} was judged pure"
        );
    }
    assert!(!module_is_pure_by_declaration(&module, |_| true, |_| None), "a module whose source is unknown was judged pure");
    let plain = source_declares_effect(&plain_src);
    assert!(module_is_pure_by_declaration(&module, |_| true, |_| Some(plain)), "control: a module of plain fns is pure");
}

