//! The typed byte IO MATRIX gate (R-bytes-01/03/09/13/15/19/25, docs/roadmap/
//! stdlib-redesign/L2-bytes.md). The family is declared as three axes — a type
//! row, a byte order, and an operation column — and every cell's name AND
//! signature is derived from them, then diffed against the compiler's own
//! interface of `bytes` in both directions: a missing cell, a cell with the
//! wrong shape, and a typed reader/writer outside the matrix (`get_u64_le`,
//! `put_f32_le_array`) are each a drift. The family can only grow a whole row
//! or column at a time, here, where the completeness rule is written down.

use almide::interface::{FunctionExport, TypeRef};

/// The type rows: (row, width in bytes, the Almide value type).
const ROWS: &[(&str, usize, &str)] = &[
    ("u8", 1, "Int"),
    ("i8", 1, "Int"),
    ("u16", 2, "Int"),
    ("i16", 2, "Int"),
    ("u32", 4, "Int"),
    ("i32", 4, "Int"),
    ("i64", 8, "Int"),
    ("f16", 2, "Float"),
    ("f32", 4, "Float"),
    ("f64", 8, "Float"),
    ("bool", 1, "Bool"),
];

/// Declared absences, so a guessed name is refused rather than added: `u64`
/// does not fit `Int` (`get_i64_*` reads the same bits), and the 1-byte rows
/// have no order suffix and no `_array` (R-bytes-19: `slice` + `to_list`).
/// Writers and cursors have no `_array` either (R-bytes-19: write one at a time).
const ABSENT_ROWS: &[&str] = &["u64"];

/// Every cell of the matrix as (name, rendered signature).
fn cells() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for &(row, width, ty) in ROWS {
        let orders: &[&str] = if width > 1 { &["_le", "_be"] } else { &[""] };
        for order in orders {
            let cell = format!("{row}{order}");
            out.push((format!("get_{cell}"), format!("(b: Bytes, pos: Int) -> {ty}?")));
            out.push((format!("next_{cell}"), format!("(b: Bytes, pos: Int) -> ({ty}?, Int)")));
            out.push((format!("put_{cell}"), format!("(mut b: Bytes, pos: Int, x: {ty}) -> Unit")));
            out.push((format!("append_{cell}"), format!("(mut b: Bytes, x: {ty}) -> Unit")));
            if width > 1 {
                out.push((
                    format!("get_{cell}_array"),
                    format!("(b: Bytes, pos: Int, count: Int) -> List[{ty}]?"),
                ));
            }
        }
    }
    out
}

fn render(t: &TypeRef) -> String {
    match t {
        TypeRef::Int => "Int".into(),
        TypeRef::Float => "Float".into(),
        TypeRef::String => "String".into(),
        TypeRef::Bool => "Bool".into(),
        TypeRef::Unit => "Unit".into(),
        TypeRef::Bytes => "Bytes".into(),
        TypeRef::Matrix => "Matrix".into(),
        TypeRef::List { inner } => format!("List[{}]", render(inner)),
        TypeRef::Option { inner } => format!("{}?", render(inner)),
        TypeRef::Tuple { elements } => {
            format!("({})", elements.iter().map(render).collect::<Vec<_>>().join(", "))
        }
        other => format!("{other:?}"),
    }
}

fn signature(f: &FunctionExport) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}{}: {}", if p.is_mut { "mut " } else { "" }, p.name, render(&p.ty)))
        .collect();
    format!("({}) -> {}", params.join(", "), render(&f.ret))
}

/// A name spelled like a typed reader/writer: an operation, a scalar row
/// token, an optional order and an optional `_array`.
fn is_typed_io_name(name: &str) -> bool {
    let Some(rest) = ["get_", "next_", "put_", "append_"].iter().find_map(|op| name.strip_prefix(op)) else {
        return false;
    };
    let rest = rest.strip_suffix("_array").unwrap_or(rest);
    let rest = rest.strip_suffix("_le").or_else(|| rest.strip_suffix("_be")).unwrap_or(rest);
    rest == "bool"
        || ABSENT_ROWS.contains(&rest)
        || (rest.len() > 1
            && matches!(&rest[..1], "u" | "i" | "f")
            && rest[1..].chars().all(|c| c.is_ascii_digit()))
}

/// The drifts between the declared matrix and `bytes`' interface.
pub(super) fn check(functions: &[FunctionExport]) -> Vec<String> {
    let mut drifts = Vec::new();
    let cells = cells();
    for (name, want) in &cells {
        match functions.iter().find(|f| &f.name == name) {
            None => drifts.push(format!("bytes IO matrix: `bytes.{name}{want}` is a cell of the matrix but missing")),
            Some(f) if &signature(f) != want => drifts.push(format!(
                "bytes IO matrix: `bytes.{name}` is `{}`, the matrix declares `{want}`",
                signature(f)
            )),
            Some(_) => {}
        }
    }
    for f in functions {
        if is_typed_io_name(&f.name) && !cells.iter().any(|(n, _)| n == &f.name) {
            drifts.push(format!(
                "bytes IO matrix: `bytes.{}` is spelled like a typed reader/writer but is not a cell of the matrix (rows {:?}, declared absent {:?})",
                f.name,
                ROWS.iter().map(|r| r.0).collect::<Vec<_>>(),
                ABSENT_ROWS
            ));
        }
    }
    drifts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix_has_every_cell_once() {
        let cells = cells();
        // 3 one-byte rows × 4 columns + 8 multi-byte rows × 2 orders × 5 columns.
        assert_eq!(cells.len(), 3 * 4 + 8 * 2 * 5);
        let mut names: Vec<&String> = cells.iter().map(|(n, _)| n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), cells.len());
    }

    #[test]
    fn the_out_of_matrix_spellings_are_recognized() {
        for n in ["get_u64_le", "put_f32_le_array", "append_u8_le", "next_i16", "get_bool"] {
            assert!(is_typed_io_name(n), "{n}");
        }
        for n in ["get", "append_bytes", "next_bytes", "get_or", "put", "append"] {
            assert!(!is_typed_io_name(n), "{n}");
        }
    }
}
