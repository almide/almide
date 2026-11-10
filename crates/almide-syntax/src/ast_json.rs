//! JSON carries expression structure, but no compiler-local expression ids.
//! Reconstruct an id for every deserialized node, including expressions in
//! record defaults and patterns that a body-only AST traversal would miss.

use super::{Expr, ExprId, ExprKind};
use serde::{Deserialize, Deserializer};
use std::sync::atomic::{AtomicU32, Ordering};

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let kind = ExprKind::deserialize(deserializer)?;
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let id = NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| serde::de::Error::custom("expression id space exhausted while decoding AST"))?;
        Ok(Expr { id: ExprId(id), span: None, kind })
    }
}
