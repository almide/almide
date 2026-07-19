//! The CO-OWN refcount trust anchor — the single source of truth for which self-host routines may
//! name the raw `prim.rc_inc` / `prim.rc_dec` Value-model primitives.
//!
//! These primitives free/acquire a refcount on a load64-fed handle that the ownership cert cannot see
//! (the differential-test floor): an UNTRACKED free exposed to arbitrary code would let any function
//! double-free outside the cert's sight. So the lowering GATES `rc_inc`/`rc_dec` to this whitelist
//! (crates/almide-mir/src/lower/calls_p4.rs) — any other `.almd` fn naming them is rejected.
//!
//! TRUST GROUNDING (柱C Brick 3). Each name here is a CO-OWN PRODUCER (rc_inc's each loaded element
//! into a fresh container) or a RECURSIVE-DROP CONSUMER (rc_dec's each element at the container's
//! death) — the two halves of the pattern proven leak/double-free-free on the Coq kernel:
//!   - proofs/CoownLoop.v `coown_fill_drop_neutral` — the producer's +1-per-element fill is returned to
//!     source by the consumer's -1-per-element recursive drop, for ANY element count.
//!   - proofs/CoownCompose.v `lifecycle_safe` — a container that is block-balanced (its own rc cert)
//!     AND child-source-owned composes to fully safe.
//! So this gate is not bare trust: a name belongs here IFF it follows that proven pattern, and a single
//! list keeps the producer/consumer halves from silently drifting apart (the `coown_names_documented`
//! test pins the set + every entry's role). Cert-PROVING each producer per-function (retiring this
//! whitelist) is the remaining Brick-3 engineering: a load64→ElementOf(container) provenance + a
//! cross-function ContainerFill↔ContainerDrop pairing that consumes CoownLoop.v — confirmed multi-week
//! (the producer fill and its recursive drop live in DIFFERENT functions, so no per-function cert
//! refinement catches the balance). See docs/roadmap/active/value-rc-cert.md.

/// CO-OWN PRODUCERS — rc_inc each loaded element to co-own it into a fresh container (the `coown_fill`
/// half of CoownLoop.v). Their balancing release is a recursive drop in [`COOWN_DROPS`].
pub const COOWN_PRODUCERS: &[&str] = &[
    "__varr_copy",        // value.array  — rc_inc each Array element (shallow copy)
    "__vfill",            // value as_array element-list fill
    "__lc_copy_rc",       // list-concat copy — co-own each appended heap element
    "__flat_copy_sub_rc", // flatten copy — the result co-owns each heap sublist slot
    "__copy_slots_rc",    // list.set_str — rc-copy each String element
    "__ldls_share",       // list.take/drop_liststr — rc_inc each shared inner list
    "value_get",          // Object linear-scan get — rc_inc the found value (caller co-owns the result)
    "__vobj_fill",        // value.object — rc_inc each key/value (shallow copy)
    "__lsv_copy",         // list.set_value — rc-copy each Value element
    "__lsv_insert_fill",  // list.insert_value — rc_inc the inserted Value
    "__ls_insert_fill",   // list.insert_str — rc_inc the inserted String
    "__lsuv_val",         // list.update_value — rc_inc each shared (non-updated) Value element
    "__lsu_val_str",      // list.update_str — rc_inc each shared (non-updated) String element
    "__sort_copy_rc",     // list.sort_str — rc-copy each String element
    "__filterrc_fill",
    "__sbfr_init",
    "__sbr_init",         // list.sort_by_rc — rc_inc each element handle copied into the result
    "__ivh_set_copy",
    "__hvl_set_copy",
    "__hvl_set_append",
    "__msv_set_copy",    // map.set_msv — rc_inc the shared value handles (map_hval's exact shape)
    "__msv_set_append",  // map.set_msv — rc_inc the appended value handle
    "__hobj_set_copy",   // map.from_list_hobj — rc_inc the shared value handles (the msv shape, opaque values)
    "__hobj_set_append", // map.from_list_hobj — rc_inc the appended value handle
    "__mlo_set_copy",    // map.set_mlo — rc_inc the shared value handles (the msv shape, list values)
    "__mlo_set_append",  // map.set_mlo — rc_inc the appended value handle
    "__ivh_drop_vals",
    "__hvl_drop_slots",    // list.filter_rc — rc_inc each KEPT non-String heap element (handle share)
    "__vmerge_fill_a",    // value.merge — rc_inc each kept/overridden key+value
    "__vmerge_app_b",     // value.merge — rc_inc each appended b key+value
    "__mx_share_fill",    // matrix.from_lists/to_lists — rc_inc each shared row block
    "__repeat_fill_rc",   // list.repeat_rc — rc_inc the element into each duplicated slot
    "__enum_fill_h",      // list.enumerate_str — rc_inc the element into its (i, x) pair
    "__zip_fill_rc",      // list.zip_rc — rc_inc both elements into each pair
    "__lpart_fill_rc",    // list.partition_rc — rc_inc each element into its side
    "__otl_fill_rc",      // option.to_list_rc — rc_inc the Some payload into the list
    "__zip_fill_rcb",     // list.zip_sh — rc_inc the heap RIGHT element only
    "__zip_fill_rca",     // list.zip_hs — rc_inc the heap LEFT element only
    "__take_h_fill",      // list.take_hshare — rc_inc each shared element slot
    "__uh_acquire",       // list.unique_hshare / dedup_hshare — rc_inc each KEPT shared element
    "__skv_entries_fill", // map.entries_skv — rc_inc each key into its (k, v) pair
    "__vu_fill",          // value.pick/omit — rc_inc each kept key+value into the fresh Object
    "__vu_ren_fill_c",    // value.to_camel_case — rc_inc each value (keys are fresh owned strings)
    "__vu_ren_fill_s",    // value.to_snake_case — rc_inc each value (keys are fresh owned strings)
    "__ordc_copy_rc",     // list.sort_{tss,tsstr,lint,oint} — rc-copy each compound element (the __sort_copy_rc shape)
    "__ordc_some_tss",    // list.min/max_tss — rc_inc the winning element into the fresh Some (the __otl_fill_rc shape)
    "__ordc_some_tsstr",  // list.min/max_tsstr — same share-into-Some
    "__ordc_some_lint",   // list.min/max_lint — same share-into-Some
    "__ordc_some_lstr",   // list.min/max_lstr — same share-into-Some
    "__ordc_some_oint",   // list.min/max_oint — same share-into-Some
    "__mtc_entries_hvalt_fill", // map.entries_hvalt — rc_inc key + flat tuple value into each (k, v) pair
];

/// RECURSIVE-DROP CONSUMERS — rc_dec each element at the container's death (the `rec_drop` half of
/// CoownLoop.v). Plus every generated `__drop_*` (per-type ADT recursive frees), admitted by prefix.
pub const COOWN_DROPS: &[&str] = &[
    "__drop_value",          // Value recursive free (tag-dispatch)
    "__drop_list_value",     // List[Value] recursive free
    "__svdrop_list",         // List[(String,Value)] recursive free
    "__ssdrop_list",         // List[(String,String)] recursive free
    "__isdrop_list",         // List[(Int,String)] recursive free (list.enumerate)
    "__drop_list_str_value", // mixed list recursive free
    "__drop_result_lv",      // Result[List[Value]] recursive free
    "__vdrop_obj",           // Object recursive free (rc_dec key, __drop_value value)
];

/// BOTH a producer and a consumer in one routine — they rc_dec the replaced element AND rc_inc the new
/// one (a set/replace co-own that releases the old ref and acquires the new). Listed separately so the
/// producer↔consumer documentation stays exact.
pub const COOWN_SET_REPLACE: &[&str] = &[
    "__set_slot_str",  // list.set_str  — rc_dec replaced + rc_inc new
    "list_set_str",    // (reaches rc via the helpers above; admitted by name)
    "__lsv_set",       // list.set_value — __drop_value replaced + rc_inc new
    "list_set_value",  //
];

/// Every routine permitted to name `prim.rc_inc` / `prim.rc_dec` (the union of the three roles above).
/// `calls_p4.rs` gates on `is_coown_rc_routine(name) || name.starts_with("__drop_")`.
pub fn is_coown_rc_routine(name: &str) -> bool {
    COOWN_PRODUCERS.contains(&name)
        || COOWN_DROPS.contains(&name)
        || COOWN_SET_REPLACE.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the trust anchor: the set is single-source and every entry's role is one of the three
    /// documented kinds (producer / drop / set-replace). A drift — adding a co-own producer without a
    /// recursive-drop partner, or a stray name — forces a deliberate edit through this test, binding the
    /// whitelist to the CoownLoop.v / CoownCompose.v pattern it relies on (柱C Brick 3 grounding).
    #[test]
    fn coown_names_documented() {
        // No name appears in more than one role list (the roles are disjoint; set-replace is its own).
        for p in COOWN_PRODUCERS {
            assert!(!COOWN_DROPS.contains(p), "{p} is in both PRODUCERS and DROPS");
            assert!(!COOWN_SET_REPLACE.contains(p), "{p} is in both PRODUCERS and SET_REPLACE");
        }
        for d in COOWN_DROPS {
            assert!(!COOWN_SET_REPLACE.contains(d), "{d} is in both DROPS and SET_REPLACE");
        }
        // The anchor is non-empty in each role (the pattern needs producers AND drops to compose).
        assert!(!COOWN_PRODUCERS.is_empty() && !COOWN_DROPS.is_empty());
        // Every recursive-drop name starts with `__` (a trusted internal) — never a public stdlib name
        // that arbitrary code could call to reach a raw free.
        for d in COOWN_DROPS {
            assert!(d.starts_with("__"), "drop {d} must be an internal __ routine");
        }
        // The union accessor agrees with the role lists (no name admitted that isn't documented).
        for name in COOWN_PRODUCERS.iter().chain(COOWN_DROPS).chain(COOWN_SET_REPLACE) {
            assert!(is_coown_rc_routine(name), "{name} not admitted by is_coown_rc_routine");
        }
    }
}
