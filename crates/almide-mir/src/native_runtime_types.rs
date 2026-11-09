//! Opaque stdlib values keep their native runtime representation. In
//! particular an HTTP response owns raw bytes and a call owns a cancellation
//! handle; neither can be replaced by the wasm self-host representation.
use crate::render_native::NTy;
use crate::render_native_rec as rec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeType(pub &'static str);

impl RuntimeType {
    pub(crate) fn of(name: &str) -> Option<Self> {
        use almide_lang::stdlib_info::{runtime_backed_type_owner, RUNTIME_BACKED_TYPES};
        let owner = runtime_backed_type_owner(name)?;
        let bare = name.rsplit('.').next()?;
        RUNTIME_BACKED_TYPES.iter().find_map(|(m, n)|
            (*m == owner && *n == bare).then_some(Self(n)))
    }

    /// The reserved runtime namespace, validated against each intrinsic's
    /// actual Rust signature before the floor admits a call.
    pub(crate) fn rust(self) -> &'static str {
        static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        let types = almide_lang::stdlib_info::RUNTIME_BACKED_TYPES;
        let names = NAMES.get_or_init(|| types.iter().map(|(_, n)| format!("Almide{n}")).collect());
        names[types.iter().position(|(_, n)| *n == self.0).expect("declared runtime type")].as_str()
    }

    pub(crate) fn kind(self) -> NTy {
        NTy::Rec(rec::runtime_type_index(self))
    }

    pub(crate) fn accepts(self, got: NTy) -> bool {
        matches!(got, NTy::Rec(i) | NTy::RecRef(i) if rec::runtime_type_of(i) == Some(self))
    }
}
