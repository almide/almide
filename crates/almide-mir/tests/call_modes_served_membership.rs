//! #4828: the call-modes witness `almide verify --mir` emits admits an
//! out-of-program callee (no row) only by membership in what the render
//! serves. A name the lowering minted that no render serves — here the
//! `__repr_<Type>` routine a derived `Repr` lowers to, which no product path
//! generates since the incumbent generators were deleted — gets the
//! out-of-range index the checker rejects, although its spelling (a leading
//! `__`) used to admit it. The control interpolates an `Int`, whose callee
//! the render serves, and gets no out-of-range row.

/// The callee indices of the witness's site rows (`sigs|site;site;...`,
/// each site `idx mode...`).
fn site_indices(modes: &str) -> Vec<usize> {
    let sites = modes.split_once('|').map(|(_, s)| s).unwrap_or("");
    sites
        .split(';')
        .filter_map(|row| row.split_whitespace().next())
        .filter_map(|idx| idx.parse().ok())
        .collect()
}

fn out_of_range_rows(src: &str) -> (usize, String) {
    let w = almide_mir::pipeline::program_witnesses(src, &[], None).expect("the program lowers");
    let modes = w.call_modes.clone().expect("a non-empty program has a call-modes witness");
    assert!(
        w.functions.iter().any(|f| f.name == "show"),
        "`show` must lower, not wall (a walled callee would be the rejected row instead): {:?}",
        w.walled
    );
    let unknown = w.functions.len();
    (site_indices(&modes).into_iter().filter(|&i| i == unknown).count(), modes)
}

#[test]
fn an_unserved_wall_callee_is_rejected_by_the_call_modes_witness() {
    let (rows, modes) = out_of_range_rows(
        "type Kind: Eq, Repr = | Hot | Cold\n\
         fn show(k: Kind) -> String = k.repr()\n\
         effect fn main() -> Unit = println(show(Hot))\n",
    );
    assert!(rows > 0, "the unserved wall callee must get the out-of-range row: `{modes}`");
}

#[test]
fn a_served_callee_gets_no_out_of_range_row() {
    let (rows, modes) = out_of_range_rows(
        "fn show(n: Int) -> String = \"${n}\"\n\
         effect fn main() -> Unit = println(show(1))\n",
    );
    assert_eq!(rows, 0, "every callee here is served: `{modes}`");
}
