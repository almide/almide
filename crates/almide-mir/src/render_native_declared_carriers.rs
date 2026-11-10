// Preserve scalar carrier declarations on payload reads, where the native
// type table deliberately has no generated record entry for these carriers.
fn declared_scalar_carriers() -> &'static std::collections::BTreeMap<String, NTy> {
    static KINDS: std::sync::OnceLock<std::collections::BTreeMap<String, NTy>> = std::sync::OnceLock::new();
    KINDS.get_or_init(|| {
        use almide_lang::types::Ty;
        let mut kinds = std::collections::BTreeMap::new();
        for payload in [Ty::Unit, Ty::String, Ty::Bool, Ty::Int, Ty::Int8, Ty::Int16, Ty::Int32, Ty::Int64,
            Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64, Ty::Float, Ty::Float32, Ty::Float64] {
            for ty in [Ty::option(payload.clone()), Ty::result(payload, Ty::String)] {
                if let Some(kind) = scalar_carrier_kind(&ty) { kinds.insert(block_key(&ty), kind); }
            }
        }
        kinds
    })
}

#[cfg(test)]
mod declared_scalar_carrier_tests {
    use super::*;
    #[test]
    fn carrier_metadata_retains_float_bits_and_both_result_types() {
        use almide_lang::types::Ty;
        for t in [Ty::Bool, Ty::Int, Ty::Int8, Ty::Int16, Ty::Int32, Ty::Int64,
            Ty::UInt8, Ty::UInt16, Ty::UInt32, Ty::UInt64, Ty::Float, Ty::Float32, Ty::Float64] {
            assert_eq!(declared_kind(&block_key(&Ty::option(t.clone()))), Some(NTy::Opt));
            assert_eq!(declared_kind(&block_key(&Ty::result(t, Ty::String))), Some(NTy::Res));
        }
        assert_eq!(declared_kind(&block_key(&Ty::result(Ty::Unit, Ty::String))), Some(NTy::Res));
        assert_eq!(declared_kind(&block_key(&Ty::result(Ty::String, Ty::String))), Some(NTy::ResStr));
        assert!(!declared_scalar_carriers().contains_key(&block_key(&Ty::result(Ty::String, Ty::Int))));
        assert!(!declared_scalar_carriers().contains_key(&block_key(&Ty::option(Ty::String))));
    }
}
