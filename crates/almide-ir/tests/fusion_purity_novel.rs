//! purity.fusion-effect-modules (#4855, ruling 2026-11-20): a pipeline stage
//! calling a stdlib module may fuse only when that module's own source
//! declares no effect. No module is named in the decision, so the test
//! enumerates EVERY bundled module at run time (a module added later is
//! covered without editing this file) and adds a fresh module name no
//! registry has: unknown is not pure.
#[path = "../../../tests/support/novel.rs"]
mod novel;

use almide_ir::fusion::stdlib_module_is_pure;
use almide_lang::stdlib_info::{bundled_module_declares_effect, BUNDLED_MODULES};

#[test]
fn novel_fusion_purity_is_the_declared_effect_set() {
    let mut effectful = 0;
    for &m in BUNDLED_MODULES {
        let declared = bundled_module_declares_effect(m);
        if declared != Some(false) {
            effectful += 1;
            assert!(!stdlib_module_is_pure(m), "`{m}` declares an effect (or has no readable source) yet fuses as pure");
        }
    }
    assert!(effectful > 0, "the enumeration saw no effect-declaring module: it did not run");
    // the declaration, not a name, decides: `io` declares `effect fn print`
    assert!(!stdlib_module_is_pure("io"));
    // a module no registry has is unknown, so it never fuses
    assert!(!stdlib_module_is_pure(&novel::module()));
    // control: a module whose source declares no effect stays fusable
    assert!(stdlib_module_is_pure("list"), "control: `list` declares no effect and must stay fusable");
}
