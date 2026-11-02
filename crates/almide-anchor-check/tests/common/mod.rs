//! What the anchor-check tests share.

/// Function 0 is `$alloc`; function 1 is the subject, its body `body`.
pub fn module(locals: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main {locals} {body}))"#
    ))
    .expect("wat")
}
