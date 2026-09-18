// Argument, tuple, and record members whose comments cannot stay inline need
// physical lines; an inline `/* */` leaves the container on one line (#1404).
fn fmt_commented_members(out: &mut String, members: &[(Option<&str>, &Expr)], close: char, depth: usize) -> bool {
    if !members.iter().any(|(_, e)| has_own_line_leading_comments(e.id) || has_continuation_comments(e.id)) { return false; }
    out.push('\n');
    for (name, expr) in members {
        emit_leading_comment_lines(out, expr.id, depth + 1);
        out.push_str(&ind(depth + 1));
        match name {
            Some("...") => out.push_str("..."),
            Some(name) => w!(out, "{name}: "),
            None => {}
        }
        fmt_expr_sans_leading(out, expr, depth + 1);
        out.push(',');
        if let Some(comments) = comments_for(expr.id) {
            for comment in &comments.line_trailing { w!(out, " {comment}"); }
            out.push('\n');
            for comment in &comments.line_between { wln!(out, "{}{}", ind(depth + 1), comment); }
        } else { out.push('\n'); }
    }
    out.push_str(&ind(depth));
    out.push(close);
    true
}
