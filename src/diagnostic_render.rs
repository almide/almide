/// Diagnostic rendering: display, source-annotated display, and JSON output.
///
/// Moved from almide-base so that rendering logic (colors, formatting) lives
/// in the CLI binary, while the foundation crate keeps only data + constructors.

use std::sync::atomic::{AtomicBool, Ordering};
use almide_base::diagnostic::{Diagnostic, Level};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

/// Call once at startup to enable colors if stderr is a TTY.
pub fn init_color() {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        COLOR_ENABLED.store(true, Ordering::Relaxed);
    }
}

fn use_color() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

// ANSI codes
const RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[1;33m";
const CYAN: &str = "\x1b[1;36m";
const BLUE: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

pub fn display(d: &Diagnostic) -> String {
    let color = use_color();
    let (prefix_color, prefix) = match d.level {
        Level::Error => (RED, "error"),
        Level::Warning => (YELLOW, "warning"),
    };
    let code_str = d.code.map(|c| format!("[{}]", c)).unwrap_or_default();
    let mut out = if color {
        format!("{}{}{}{}: {}{}{}", prefix_color, prefix, code_str, RESET, BOLD, d.message, RESET)
    } else {
        format!("{}{}: {}", prefix, code_str, d.message)
    };
    match (&d.file, d.line) {
        (Some(f), Some(l)) => {
            let loc = match d.col {
                Some(c) => format!("{}:{}:{}", f, l, c),
                None => format!("{}:{}", f, l),
            };
            let arrow = if color { format!("{}-->{}", BLUE, RESET) } else { "-->".into() };
            out.push_str(&format!("\n  {} {}", arrow, loc));
        }
        (Some(f), None) => {
            let arrow = if color { format!("{}-->{}", BLUE, RESET) } else { "-->".into() };
            out.push_str(&format!("\n  {} {}", arrow, f));
        }
        (None, Some(l)) => out.push_str(&format!("\n  at line {}", l)),
        _ => {}
    }
    if !d.context.is_empty() {
        let line = if color { format!("{}in {}{}", DIM, d.context, RESET) } else { format!("in {}", d.context) };
        out.push_str(&format!("\n  {}", line));
    }
    if let Some(here) = &d.here_snippet {
        if !here.is_empty() {
            let line = if color { format!("{}here:{} {}", CYAN, RESET, here) } else { format!("here: {}", here) };
            out.push_str(&format!("\n  {}", line));
        }
    }
    if !d.hint.is_empty() {
        let line = if color { format!("{}hint:{} {}", CYAN, RESET, d.hint) } else { format!("hint: {}", d.hint) };
        out.push_str(&format!("\n  {}", line));
    }
    if let Some(snippet) = &d.try_snippet {
        let label = if color { format!("{}try:{}", CYAN, RESET) } else { "try:".to_string() };
        out.push_str(&format!("\n  {}", label));
        for sline in snippet.lines() {
            out.push_str(&format!("\n      {}", sline));
        }
    }
    out
}

pub fn to_json(d: &Diagnostic) -> String {
    let level = match d.level { Level::Error => "error", Level::Warning => "warning" };
    let code = d.code.unwrap_or("");
    let file = d.file.as_deref().unwrap_or("");
    let line = d.line.unwrap_or(0);
    let col = d.col.unwrap_or(0);
    let end_col = match d.end_col {
        Some(c) => c.to_string(),
        None => "null".to_string(),
    };
    let secondary_items: Vec<String> = d.secondary.iter().map(|s| {
        let s_col = match s.col {
            Some(c) => c.to_string(),
            None => "null".to_string(),
        };
        format!(
            r#"{{"line":{},"col":{},"label":"{}"}}"#,
            s.line, s_col, s.label.replace('"', r#"\""#),
        )
    }).collect();
    let secondary = format!("[{}]", secondary_items.join(","));
    let here_json = match &d.here_snippet {
        Some(s) => format!(
            "\"{}\"",
            s.replace('"', r#"\""#).replace('\n', "\\n")
        ),
        None => "null".to_string(),
    };
    let try_json = match &d.try_snippet {
        Some(s) => format!(
            "\"{}\"",
            s.replace('"', r#"\""#).replace('\n', "\\n")
        ),
        None => "null".to_string(),
    };
    // Manual JSON to avoid serde dependency in this module
    format!(
        r#"{{"level":"{}","code":"{}","message":"{}","hint":"{}","here":{},"try":{},"context":"{}","file":"{}","line":{},"col":{},"end_col":{},"secondary":{}}}"#,
        level, code,
        d.message.replace('"', r#"\""#).replace('\n', "\\n"),
        d.hint.replace('"', r#"\""#).replace('\n', "\\n"),
        here_json, try_json,
        d.context.replace('"', r#"\""#),
        file.replace('"', r#"\""#),
        line, col, end_col, secondary,
    )
}

pub fn display_with_source(d: &Diagnostic, source: &str) -> String {
    let color = use_color();
    let source_lines: Vec<&str> = source.lines().collect();

    // Auto-populate `here_snippet` from the primary span's source line
    // (if not already set) so plain `display()` consumers still see the
    // inline `here:` row. The gutter-formatted source below is the
    // full Here/Try/Hint triple's visual context.
    let mut enriched = d.clone();
    if enriched.here_snippet.is_none() {
        if let Some(line_num) = d.line {
            if let Some(src) = source_lines.get(line_num.saturating_sub(1)) {
                let trimmed = src.trim();
                if !trimmed.is_empty() {
                    enriched.here_snippet = Some(trimmed.to_string());
                }
            }
        }
    }
    let d = &enriched;
    let mut out = display(d);

    // Render secondary spans first (declaration sites, etc.)
    for sec in &d.secondary {
        let Some(src_line) = source_lines.get(sec.line.saturating_sub(1)) else { continue; };
        let trimmed = src_line.trim_end();
        if trimmed.is_empty() { continue; }
        let max_line = d.line.unwrap_or(sec.line).max(sec.line);
        let width = format!("{}", max_line).len();
        let gutter_pad = " ".repeat(width);
        if color {
            out.push_str(&format!("\n{}{} {}|{}", gutter_pad, BLUE, RESET, BLUE));
            out.push_str(&format!("\n{}{:>width$}{} {}|{} {}",
                BLUE, sec.line, RESET, BLUE, RESET, trimmed, width = width));
        } else {
            out.push_str(&format!("\n{} |", gutter_pad));
            out.push_str(&format!("\n{:>width$} | {}", sec.line, trimmed, width = width));
        }
        // Dash underline with label for secondary
        let Some(col) = sec.col else { continue; };
        let col0 = col.saturating_sub(1);
        let dash_len = if !sec.label.is_empty() { sec.label.len().max(1) } else { 1 };
        let pad = " ".repeat(col0);
        let dashes = "-".repeat(dash_len);
        let label_suffix = if sec.label.is_empty() { String::new() } else if color {
            format!(" {}{}{}", CYAN, sec.label, RESET)
        } else {
            format!(" {}", sec.label)
        };
        if color {
            out.push_str(&format!("\n{}{} {}|{} {}{}{}{}{}{}",
                gutter_pad, BLUE, RESET, BLUE, RESET,
                pad, CYAN, dashes, RESET, label_suffix));
        } else {
            out.push_str(&format!("\n{} | {}{}{}", gutter_pad, pad, dashes, label_suffix));
        }
    }

    // Render primary span
    let Some(line_num) = d.line else { return out; };
    let Some(source_line) = source_lines.get(line_num.saturating_sub(1)) else { return out; };
    let trimmed = source_line.trim_end();
    if trimmed.is_empty() { return out; }
    let width = format!("{}", line_num).len();
    let gutter_pad = " ".repeat(width);
    // Separator between secondary and primary if they exist
    if d.secondary.is_empty() || d.secondary.iter().all(|s| s.line == line_num) {
        if color {
            out.push_str(&format!("\n{}{} {}|{}", gutter_pad, BLUE, RESET, BLUE));
        } else {
            out.push_str(&format!("\n{} |", gutter_pad));
        }
    } else {
        // Ellipsis between distant spans
        let ellipsis_pad = " ".repeat(width.saturating_sub(2));
        if color {
            out.push_str(&format!("\n{}{}...{}", BLUE, ellipsis_pad, RESET));
        } else {
            out.push_str(&format!("\n{}...", ellipsis_pad));
        }
    }
    if color {
        out.push_str(&format!("\n{}{:>width$}{} {}|{} {}",
            BLUE, line_num, RESET, BLUE, RESET, trimmed, width = width));
    } else {
        out.push_str(&format!("\n{:>width$} | {}", line_num, trimmed, width = width));
    }
    // Caret underline
    let Some(col) = d.col else { return out; };
    let col0 = col.saturating_sub(1);
    let caret_len = match d.end_col {
        Some(end_col) => { let end0 = end_col.saturating_sub(1); if end0 > col0 { end0 - col0 } else { 1 } }
        None => if !d.context.is_empty() { d.context.len().max(1) } else { 1 },
    };
    let pad = " ".repeat(col0);
    let carets = "^".repeat(caret_len);
    let (caret_color, caret_reset) = if color {
        match d.level {
            Level::Error => (RED, RESET),
            Level::Warning => (YELLOW, RESET),
        }
    } else {
        ("", "")
    };
    if color {
        out.push_str(&format!("\n{}{} {}|{} {}{}{}{}",
            gutter_pad, BLUE, RESET, BLUE, RESET,
            pad, caret_color, carets));
        out.push_str(caret_reset);
    } else {
        out.push_str(&format!("\n{} | {}{}", gutter_pad, pad, carets));
    }
    out
}
