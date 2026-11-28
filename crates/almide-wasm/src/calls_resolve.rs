//! Named-call resolution: the table entry a Named, method-suffix or
//! qualified stdlib call lowers to — split from calls.rs for the file budget.

use crate::emitter::Emitter;
use crate::*;

use super::{VERIFIED, VERIFIED_SUM_BUILDERS};

impl Emitter<'_> {
    /// The table entry a Named call to `name` lowers to — the one
    /// resolution `lower_call_at` uses, shared with the Assign release rule
    /// (#2616) so the two cannot resolve the same call differently.
    /// Intra-module Named calls resolve within the CURRENT module first
    /// (simple names collide across modules), then the entry program's
    /// globals. Codec splices resolve by BARE name through the same
    /// registry/whitelist path module calls use. A cross-module convention
    /// method (`Type.method` defined beside its type in ANOTHER module)
    /// resolves by SUFFIX when exactly one module defines it (unique-or-wall:
    /// the #1558/#1087 bare-name landmine family demands the ambiguity case
    /// refuse, never guess).
    pub(crate) fn resolve_named_fn(&self, name: &str) -> Option<usize> {
        // #4886: a derived method of a module type is named `acct.User.decode`
        // (key `acct.acct.User.decode`); it beats the entry's bare same name.
        self.cur_module
            .and_then(|m| {
                self.table.by_name.get(&format!("{m}.{name}")).or_else(|| {
                    name.contains('.').then(|| self.table.by_name.get(&format!("{m}.{m}.{name}"))).flatten()
                })
            })
            .or_else(|| self.table.by_name.get(name))
            .copied()
            .or_else(|| self.resolve_qualified(name))
            .or_else(|| self.resolve_method_suffix(name))
    }

    /// A `Type.method` spelling that missed both the current module and
    /// the bare table: accept the module-qualified key ENDING in
    /// `.Type.method` iff it is UNIQUE across modules — ambiguity walls
    /// (order-independent: uniqueness needs no iteration order).
    pub(crate) fn resolve_method_suffix(&self, name: &str) -> Option<usize> {
        if !name.contains('.') {
            return None;
        }
        let suffix = format!(".{name}");
        let mut hits = self.table.by_name.iter().filter(|(k, _)| k.ends_with(&suffix));
        let first = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        Some(*first.1)
    }

    /// Resolve a qualified stdlib call ("float.to_string") to a table
    /// index: linked module fns first, then the self-host registry's
    /// implementation index.
    pub(crate) fn resolve_qualified(&self, key: &str) -> Option<usize> {
        self.table.by_name.get(key).copied().or_else(|| {
            let impl_fn = almide_types::self_host_registry::self_host_runtime()
                .iter()
                .flat_map(|(_, maps)| maps.iter())
                .find(|(_, surface)| *surface == key)
                .map(|(impl_fn, _)| *impl_fn)?;
            // The local tiers first, then the audited families (whitelist.rs).
            let exempt = if VERIFIED.contains(&impl_fn) {
                false
            } else if VERIFIED_SUM_BUILDERS.contains(&impl_fn) {
                true
            } else {
                crate::whitelist::tier_of(impl_fn)?
            };
            let i = self.table.impl_index.get(impl_fn).copied()?;
            // LAYOUT BOUNDARY: self-host impls encode the INCUMBENT's
            // block layout. Scalars, strings and List[scalar] match our
            // ratified layout byte-for-byte; sums/maps/sets/tuples/named
            // do NOT (the incumbent keeps the Result tag in the len slot
            // — found by the burn-up: result.unwrap_or(ok(5)) returned
            // the default). An impl whose signature carries a
            // layout-coupled type stays UNRESOLVED (honest wall) until
            // the layouts are deliberately reconciled.
            let info = &self.table.infos[i];
            let coupled = |t: &SliceTy| {
                match t {
                    SliceTy::Option(_)
                    | SliceTy::Result(..)
                    | SliceTy::Map(..)
                    | SliceTy::Set(_)
                    | SliceTy::Tuple(_)
                    | SliceTy::Named(_) => true,
                    // The incumbent packs EVERY list element into an
                    // 8-byte slot; ours are 4 for the i32 word class —
                    // List[Int]/List[Float] agree, List[String]/List[Bool]
                    // do not (list.join through a linked impl trapped).
                    SliceTy::List(h) => self.types.el(*h).slot_size() == 4,
                    _ => false,
                }
            };
            if !exempt && (info.params.iter().any(coupled) || info.ret.as_ref().is_some_and(coupled))
            {
                return None;
            }
            Some(i)
        })
    }
}
