/// Almide lexer: Source text → Token stream.
///
/// Input:    &str (source code)
/// Output:   Vec<Token> (with line/col positions)
/// Owns:     character classification, string interpolation detection, keyword resolution
/// Does NOT: error recovery, syntax structure, semantic meaning
///
/// Principles:
/// 1. Single pass, no backtracking
/// 2. String interpolation handled inline
/// 3. Keywords resolved after identifier scan

// ── Token types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    // Literals
    Int, Float, String, InterpolatedString,
    // Identifiers
    Ident, TypeName,
    // Keywords
    Module, Import, Type, Protocol, For, In, Fn, Let, Var, Mut,
    If, Then, Else, Match, Ok, Err, Some, None, Todo,
    True, False, Not, And, Or,
    Strict, Pub, Effect, Test,
    Guard, Break, Continue, While, Local, Mod, Fan,
    // Delimiters
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    LAngle, RAngle,
    Comma, Dot, Colon, Semicolon,
    // Operators
    Arrow,     // ->
    FatArrow,  // =>
    Eq,        // =
    EqEq,      // ==
    Bang,      // !
    BangEq,    // !=
    LtEq,      // <=
    GtEq,      // >=
    Plus, Minus, Star, Slash, Percent,
    PlusPlus,  // ++
    Pipe,      // |
    PipeArrow, // |>
    ComposeArrow, // >>
    Caret,     // ^
    AmpAmp,    // &&
    PipePipe,  // ||
    Underscore,
    Question,         // ?
    QuestionDot,      // ?.
    QuestionQuestion, // ??
    DotDot,    // .. — record-rest in patterns/types; RETIRED as a range operator (#966: E033 fix-it → ..<)
    DotDotEq,  // ..= — RETIRED (#966: E033 fix-it → ...)
    DotDotDot, // ... — record/list spread (prefix) AND inclusive range (infix)
    DotDotLt,  // ..< — exclusive range
    At,        // @
    // Whitespace / structure
    Comment, Newline, EOF,
    // A character no lexer rule matched (e.g. a stray full-width character
    // outside a string). Must surface as a parse error — never dropped (#1308).
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: std::string::String,
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
}

// ── Lexer ───────────────────────────────────────────────────────

pub struct Lexer;

impl Lexer {
    pub fn tokenize(src: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        // Strip a leading UTF-8 BOM (editor/Windows artifact). Without this it
        // would lex as an Unknown token and fail the whole file (#1308).
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        // Normalize CRLF → LF (Windows compatibility)
        let normalized;
        let src = if src.contains('\r') {
            normalized = src.replace("\r\n", "\n").replace('\r', "\n");
            &normalized
        } else {
            src
        };
        let chars: Vec<char> = src.chars().collect();
        let mut cur = Cursor { pos: 0, line: 1, col: 1 };
        while cur.pos < chars.len() {
            if lex_trivia(&chars, &mut cur, &mut tokens) { continue; }
            if lex_string_or_number(&chars, &mut cur, &mut tokens) { continue; }
            if lex_word(&chars, &mut cur, &mut tokens) { continue; }
            // Operators and delimiters
            let (tok, len) = lex_operator(&chars, cur.pos, cur.line, cur.col);
            tokens.push(tok);
            cur.pos += len; cur.col += len;
        }

        tokens.push(Token { token_type: TokenType::EOF, value: String::new(), line: cur.line, col: cur.col, end_col: cur.col });
        tokens
    }
}

/// The tokenize loop's scan position (line/col are 1-based).
struct Cursor { pos: usize, line: usize, col: usize }

/// Whitespace, newlines and comments. True = consumed.
fn lex_trivia(chars: &[char], cur: &mut Cursor, tokens: &mut Vec<Token>) -> bool {
    let ch = chars[cur.pos];
    // Skip whitespace (except newlines)
    if ch == ' ' || ch == '\t' || ch == '\r' {
        cur.pos += 1; cur.col += 1;
        return true;
    }
    if ch == '\n' {
        tokens.push(Token { token_type: TokenType::Newline, value: String::new(), line: cur.line, col: cur.col, end_col: cur.col + 1 });
        cur.pos += 1; cur.line += 1; cur.col = 1;
        return true;
    }
    // Line comment
    if ch == '/' && peek(chars, cur.pos + 1) == Some('/') {
        let (tok, new_pos) = lex_line_comment(chars, cur.pos, cur.line, cur.col);
        cur.col += new_pos - cur.pos;
        cur.pos = new_pos;
        tokens.push(tok);
        return true;
    }
    // Block comment /* ... */ — nestable, lexed VERBATIM into a Comment token
    // (#1318: it used to be fully skipped, so fmt could never reprint it and
    // silently deleted every block comment). The parser drops the ones sitting
    // inline mid-expression (Parser::new's trivia filter), keeping the grammar
    // unchanged; own-line and end-of-line block comments ride the same
    // comment_map rails as `//` comments.
    if ch == '/' && peek(chars, cur.pos + 1) == Some('*') {
        let start = cur.pos;
        let (start_line, start_col) = (cur.line, cur.col);
        let (p, l, c) = skip_block_comment(chars, cur.pos, cur.line, cur.col);
        let text: String = chars[start..p].iter().collect();
        tokens.push(Token {
            token_type: TokenType::Comment,
            value: text,
            line: start_line,
            col: start_col,
            end_col: c,
        });
        cur.pos = p; cur.line = l; cur.col = c;
        return true;
    }
    false
}

/// String literals (raw / double / single quote) and numbers — the
/// multi-line-capable lexers that manage their own line/col. True = consumed.
fn lex_string_or_number(chars: &[char], cur: &mut Cursor, tokens: &mut Vec<Token>) -> bool {
    let ch = chars[cur.pos];
    // Raw string literal: r"..." or r"""..."""
    let string_lexer = if ch == 'r' && peek(chars, cur.pos + 1) == Some('"') {
        Some(lex_raw_string as fn(&[char], usize, usize, usize) -> (Token, usize, usize, usize))
    } else if ch == '"' {
        Some(lex_string as fn(&[char], usize, usize, usize) -> (Token, usize, usize, usize))
    } else if ch == '\'' {
        Some(lex_single_quote_string as fn(&[char], usize, usize, usize) -> (Token, usize, usize, usize))
    } else {
        Option::None
    };
    if let Some(lexer) = string_lexer {
        let (tok, new_pos, new_line, new_col) = lexer(chars, cur.pos, cur.line, cur.col);
        tokens.push(tok);
        cur.pos = new_pos; cur.line = new_line; cur.col = new_col;
        return true;
    }
    if ch.is_ascii_digit() {
        let (tok, new_pos) = lex_number(chars, cur.pos, cur.line, cur.col);
        cur.col += new_pos - cur.pos;
        cur.pos = new_pos;
        tokens.push(tok);
        return true;
    }
    false
}

/// Identifiers, keywords and backtick-escaped identifiers. True = consumed.
fn lex_word(chars: &[char], cur: &mut Cursor, tokens: &mut Vec<Token>) -> bool {
    let ch = chars[cur.pos];
    // Backtick-escaped identifier (`protocol`, `type`) — keywords usable as
    // identifiers, Swift-style.
    let is_backtick = ch == '`';
    // Identifier or keyword (lone `_` falls through to operator lexing → Underscore)
    let is_ident_start = ch.is_ascii_alphabetic()
        || (ch == '_' && cur.pos + 1 < chars.len() && (chars[cur.pos + 1].is_ascii_alphanumeric() || chars[cur.pos + 1] == '_'));
    if !is_backtick && !is_ident_start {
        return false;
    }
    let (tok, new_pos) = if is_backtick {
        lex_backtick_ident(chars, cur.pos, cur.line, cur.col)
    } else {
        lex_ident(chars, cur.pos, cur.line, cur.col)
    };
    cur.col += new_pos - cur.pos;
    cur.pos = new_pos;
    tokens.push(tok);
    true
}

// ── Line comment lexing ─────────────────────────────────────────

fn lex_line_comment(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start;
    while pos < chars.len() && chars[pos] != '\n' { pos += 1; }
    let text: String = chars[start..pos].iter().collect();
    let end_col = col + (pos - start);
    (Token { token_type: TokenType::Comment, value: text, line, col, end_col }, pos)
}

// ── Block comment skipping ──────────────────────────────────────

/// Skip a nestable block comment /* ... */. Returns (new_pos, new_line, new_col).
fn skip_block_comment(chars: &[char], start: usize, line: usize, col: usize) -> (usize, usize, usize) {
    let mut pos = start + 2;
    let mut ln = line;
    let mut cl = col + 2;
    let mut depth = 1;

    while pos < chars.len() && depth > 0 {
        if chars[pos] == '/' && peek(chars, pos + 1) == Some('*') {
            depth += 1; pos += 2; cl += 2;
        } else if chars[pos] == '*' && peek(chars, pos + 1) == Some('/') {
            depth -= 1; pos += 2; cl += 2;
        } else if chars[pos] == '\n' {
            ln += 1; cl = 1; pos += 1;
        } else {
            cl += 1; pos += 1;
        }
    }

    (pos, ln, cl)
}

// ── Raw string lexing ───────────────────────────────────────────

/// Lex a raw string: r"..." or r"""...""". Returns (token, new_pos, new_line, new_col).
fn lex_raw_string(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    let mut pos = start + 1; // skip 'r'
    let mut ln = line;
    let mut cl = col + 1;

    // Check for triple-quote r"""..."""
    if peek(chars, pos + 1) == Some('"') && peek(chars, pos + 2) == Some('"') {
        pos += 3; cl += 3; // skip """
        let content_start = pos;
        while pos + 2 < chars.len() && !(chars[pos] == '"' && chars[pos + 1] == '"' && chars[pos + 2] == '"') {
            if chars[pos] == '\n' { ln += 1; cl = 1; } else { cl += 1; }
            pos += 1;
        }
        let value: String = chars[content_start..pos].iter().collect();
        if pos + 2 < chars.len() { pos += 3; cl += 3; } // skip closing """
        let tok = Token { token_type: TokenType::String, value, line, col, end_col: cl };
        (tok, pos, ln, cl)
    } else {
        // Single-quote r"..."
        let content_start = pos + 1;
        pos += 1; cl += 1; // skip opening "
        while pos < chars.len() && chars[pos] != '"' {
            if chars[pos] == '\n' { ln += 1; cl = 1; } else { cl += 1; }
            pos += 1;
        }
        let value: String = chars[content_start..pos].iter().collect();
        if pos < chars.len() { pos += 1; cl += 1; } // skip closing "
        let tok = Token { token_type: TokenType::String, value, line, col, end_col: cl };
        (tok, pos, ln, cl)
    }
}

// ── String lexing ───────────────────────────────────────────────

fn lex_string(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    // Check for triple-quote heredoc: """..."""
    if start + 2 < chars.len() && chars[start + 1] == '"' && chars[start + 2] == '"' {
        return lex_heredoc(chars, start, line, col);
    }

    let mut pos = start + 1; // skip opening "
    let mut value = String::new();
    let mut has_interpolation = false;
    let mut pair_starts: Vec<usize> = Vec::new();

    while pos < chars.len() && chars[pos] != '"' {
        pos = lex_string_char(chars, pos, &mut value, &mut has_interpolation, &mut pair_starts);
    }
    if pos < chars.len() { pos += 1; } // skip closing "

    let value = if has_interpolation { value } else { strip_escape_pairs(value, &pair_starts) };
    let tt = if has_interpolation { TokenType::InterpolatedString } else { TokenType::String };
    let len = pos - start;
    let end_col = col + len;
    (Token { token_type: tt, value, line, col, end_col }, pos, line, end_col)
}

/// Single-quote string: `'...'` — no interpolation.
/// Supports escape sequences (`\\`, `\'`, `\n`, `\t`, `\r`, `\xNN`, `\u{...}`).
fn lex_single_quote_string(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    let mut pos = start + 1; // skip opening '
    let mut value = String::new();

    while pos < chars.len() && chars[pos] != '\'' {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            if let Some((c, new_pos)) = lex_numeric_escape(chars, pos) {
                value.push(c);
                pos = new_pos;
                continue;
            }
            let next = chars[pos + 1];
            match next {
                '\'' => { value.push('\''); pos += 2; }
                '\\' => { value.push('\\'); pos += 2; }
                'n' => { value.push('\n'); pos += 2; }
                't' => { value.push('\t'); pos += 2; }
                'r' => { value.push('\r'); pos += 2; }
                _ => { value.push(chars[pos]); pos += 1; }
            }
        } else {
            value.push(chars[pos]);
            pos += 1;
        }
    }
    if pos < chars.len() { pos += 1; } // skip closing '

    let len = pos - start;
    let end_col = col + len;
    (Token { token_type: TokenType::String, value, line, col, end_col }, pos, line, end_col)
}

fn lex_heredoc(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    let mut pos = start + 3; // skip opening """
    let mut raw = String::new();
    let mut has_interpolation = false;
    let mut pair_starts: Vec<usize> = Vec::new();
    let mut cur_line = line;
    let mut cur_col = col + 3;

    // Consume until closing """
    while pos + 2 < chars.len() && !(chars[pos] == '"' && chars[pos + 1] == '"' && chars[pos + 2] == '"') {
        if chars[pos] == '\n' {
            cur_line += 1;
            cur_col = 1;
        } else {
            cur_col += 1;
        }
        pos = lex_string_char(chars, pos, &mut raw, &mut has_interpolation, &mut pair_starts);
    }
    if pos + 2 < chars.len() {
        cur_col += 3; // closing """
        pos += 3;
    }

    let raw = if has_interpolation { raw } else { strip_escape_pairs(raw, &pair_starts) };
    let value = strip_heredoc_indent(&raw);
    let tt = if has_interpolation { TokenType::InterpolatedString } else { TokenType::String };
    (Token { token_type: tt, value, line, col, end_col: cur_col }, pos, cur_line, cur_col)
}

/// Process one character (or escape / interpolation) inside a string body.
/// Returns the new position after consuming the character(s).
fn lex_string_char(chars: &[char], pos: usize, buf: &mut String, has_interpolation: &mut bool, pair_starts: &mut Vec<usize>) -> usize {
    if chars[pos] == '\\' && pos + 1 < chars.len() {
        lex_escape(chars, pos, buf, pair_starts)
    } else if chars[pos] == '$' && pos + 1 < chars.len() && chars[pos + 1] == '{' {
        *has_interpolation = true;
        lex_interpolation(chars, pos, buf)
    } else {
        buf.push(chars[pos]);
        pos + 1
    }
}

/// Process a backslash escape sequence. Returns the new position.
///
/// `\\` and `\$` are NOT decoded here (#1076): an interpolated string's
/// value is re-scanned by the parser's `${...}` splitter, and a decoded
/// `\$` (or a decoded `\\` before a real `$`) becomes indistinguishable
/// from a live hole. The pair is kept verbatim, its buffer offset recorded
/// in `pair_starts`; a string that turns out NOT to be interpolated strips
/// the backslashes at the end (`strip_escape_pairs`), and an interpolated
/// one hands the pairs to the splitter, which decodes them in literal
/// segments.
fn lex_escape(chars: &[char], pos: usize, buf: &mut String, pair_starts: &mut Vec<usize>) -> usize {
    if let Some((c, new_pos)) = lex_numeric_escape(chars, pos) {
        buf.push(c);
        return new_pos;
    }
    match chars[pos + 1] {
        'n' => { buf.push('\n'); pos + 2 }
        't' => { buf.push('\t'); pos + 2 }
        'r' => { buf.push('\r'); pos + 2 }
        '\\' => { pair_starts.push(buf.len()); buf.push('\\'); buf.push('\\'); pos + 2 }
        '"' => { buf.push('"'); pos + 2 }
        '$' => { pair_starts.push(buf.len()); buf.push('\\'); buf.push('$'); pos + 2 }
        other => { buf.push('\\'); buf.push(other); pos + 2 }
    }
}

/// Remove the recorded escape-pair backslashes from a non-interpolated
/// string value (`\\` → `\`, `\$` → `$`). `pair_starts` holds the byte
/// offset of each pair's backslash, in ascending order.
fn strip_escape_pairs(value: String, pair_starts: &[usize]) -> String {
    if pair_starts.is_empty() { return value; }
    let mut out = String::with_capacity(value.len());
    let mut next = 0;
    for (i, ch) in value.char_indices() {
        if next < pair_starts.len() && pair_starts[next] == i {
            next += 1;
            continue;
        }
        out.push(ch);
    }
    out
}

/// Decode a numeric / Unicode escape starting at `pos` (which points at the `\`):
///   `\xNN`    — exactly two hex digits, a codepoint in 0..=0xFF (e.g. `\x1b` → ESC).
///   `\u{...}` — one to six hex digits, any Unicode scalar value (e.g. `\u{1f600}`).
/// Returns `(decoded_char, new_pos)` on a well-formed escape, or `None` otherwise
/// (a malformed escape falls through to the caller's literal-passthrough path, so
/// existing text like `\users` / `\xyz` is preserved unchanged). Codepoints that
/// are not valid Unicode scalars (surrogates, > U+10FFFF) also yield `None`.
fn lex_numeric_escape(chars: &[char], pos: usize) -> Option<(char, usize)> {
    match chars.get(pos + 1)? {
        'x' => {
            let hi = chars.get(pos + 2)?.to_digit(16)?;
            let lo = chars.get(pos + 3)?.to_digit(16)?;
            char::from_u32(hi * 16 + lo).map(|c| (c, pos + 4))
        }
        'u' => {
            if *chars.get(pos + 2)? != '{' {
                return None;
            }
            let mut i = pos + 3;
            let mut code: u32 = 0;
            let mut digits = 0;
            while let Some(&c) = chars.get(i) {
                if c == '}' {
                    break;
                }
                code = code.checked_mul(16)?.checked_add(c.to_digit(16)?)?;
                digits += 1;
                if digits > 6 {
                    return None;
                }
                i += 1;
            }
            if digits == 0 || chars.get(i) != Some(&'}') {
                return None;
            }
            char::from_u32(code).map(|c| (c, i + 1))
        }
        _ => None,
    }
}

/// Process an interpolation `${...}` block. Returns the new position.
///
/// The region is scanned quote-aware (#1073): a nested string literal —
/// `"…"` or `'…'`, escape pairs kept intact — is captured atomically, so
/// quotes and braces inside it neither end the outer string nor move the
/// brace depth (`"${x ?? "}"}"` used to close the interpolation on the
/// brace inside the literal and silently drop the rest of the line).
/// The habit spelling `\"…\"` — escaping the quotes as if still inside
/// the outer string, the prior every JS/Kotlin/Python writer carries in —
/// is normalized at capture time: the `\` before each delimiter is
/// dropped, so the sub-parser sees a well-formed literal either way.
fn lex_interpolation(chars: &[char], start: usize, buf: &mut String) -> usize {
    buf.push('$');
    buf.push('{');
    let mut pos = start + 2;
    let mut depth = 1;
    while pos < chars.len() && depth > 0 {
        let c = chars[pos];
        // `\"`-delimited nested string (outer-string escape habit).
        if c == '\\' && chars.get(pos + 1) == Some(&'"') {
            pos = scan_habit_quoted_literal(chars, pos, buf);
            continue;
        }
        // Bare nested string literal: capture atomically, escapes intact,
        // so `\"` inside it stays an escape and braces stay literal text.
        if c == '"' || c == '\'' {
            pos = scan_nested_string_literal(chars, pos, buf);
            continue;
        }
        if c == '{' { depth += 1; }
        if c == '}' { depth -= 1; }
        if depth > 0 { buf.push(c); }
        pos += 1;
    }
    buf.push('}');
    pos
}

/// Capture a `\"`-delimited nested string (the outer-string escape habit —
/// `\${x ?? \"d\"}` written as if still inside the outer quotes) as a plain
/// `"…"` literal, dropping the delimiter backslashes; `chars[pos]` is the
/// opening `\\`. Inside, escape pairs pass through untouched, and either
/// `\"` or a bare `"` closes.
fn scan_habit_quoted_literal(chars: &[char], pos: usize, buf: &mut String) -> usize {
    buf.push('"');
    let mut pos = pos + 2;
    while pos < chars.len() {
        if chars[pos] == '\\' && chars.get(pos + 1) == Some(&'"') {
            buf.push('"');
            return pos + 2;
        }
        if chars[pos] == '"' {
            buf.push('"');
            return pos + 1;
        }
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            buf.push(chars[pos]);
            buf.push(chars[pos + 1]);
            pos += 2;
            continue;
        }
        buf.push(chars[pos]);
        pos += 1;
    }
    pos
}

/// Capture a nested string literal (`"…"` / `'…'`) atomically, escape pairs
/// kept verbatim; `chars[pos]` is the opening delimiter. Pushes the literal
/// (delimiters included) onto `buf` and returns the position after the
/// closing delimiter. Shared by the lexer's interpolation scan above and
/// the parser's `${...}` splitter (`parse_interpolation_expr_part`) — the
/// two scans MUST agree on where a nested literal ends, or the splitter
/// re-introduces the brace-blindness the lexer just fixed (#1073).
pub(crate) fn scan_nested_string_literal(chars: &[char], pos: usize, buf: &mut String) -> usize {
    let quote = chars[pos];
    buf.push(quote);
    let mut pos = pos + 1;
    while pos < chars.len() {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            buf.push(chars[pos]);
            buf.push(chars[pos + 1]);
            pos += 2;
            continue;
        }
        let done = chars[pos] == quote;
        buf.push(chars[pos]);
        pos += 1;
        if done { break; }
    }
    pos
}

/// Strip common leading indentation from heredoc content.
/// - First line (after opening """) is skipped if blank
/// - Last line (before closing """) is dropped if whitespace-only
/// - Minimum indent of non-empty content lines is stripped from all lines
fn strip_heredoc_indent(raw: &str) -> String {
    let lines: Vec<&str> = raw.split('\n').collect();
    if lines.is_empty() { return String::new(); }

    // If first line is blank (content starts on next line after """), skip it
    let start = if lines[0].trim().is_empty() { 1 } else { 0 };
    // If last line is whitespace-only, drop it
    let end = if lines.len() > 1 && lines[lines.len() - 1].trim().is_empty() {
        lines.len() - 1
    } else {
        lines.len()
    };

    if start >= end { return String::new(); }

    let content = &lines[start..end];
    let indent = content.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    content.iter()
        .map(|l| {
            // `indent` is a byte length of leading whitespace measured over the
            // non-blank lines. A whitespace-only line (excluded from that min)
            // may hold multi-byte Unicode whitespace such as U+3000, so clamp
            // the cut down to a char boundary — slicing mid-codepoint panics.
            let mut cut = indent.min(l.len());
            while cut > 0 && !l.is_char_boundary(cut) {
                cut -= 1;
            }
            &l[cut..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Number lexing ───────────────────────────────────────────────

fn lex_number(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    // Radix prefixes: 0x hex, 0b binary, 0o octal (case-insensitive). The
    // prefix is claimed only when at least one digit of that radix follows —
    // a bare `0x` would otherwise reach `literals::int_value` as an empty
    // digit string and silently fold to 0 (#873). Without a digit the `0`
    // lexes alone and the letter starts an identifier, so the typo fails to
    // resolve instead of computing with the wrong value.
    if chars[start] == '0' && start + 1 < chars.len() {
        let radix = match chars[start + 1] {
            'x' | 'X' => Some(16),
            'b' | 'B' => Some(2),
            'o' | 'O' => Some(8),
            _ => None,
        };
        if let Some(radix) = radix {
            if let Some(lexed) = lex_radix_number(chars, start, line, col, radix) {
                return lexed;
            }
        }
    }

    let mut pos = scan_digit_run(chars, start);
    let (tail_pos, is_float) = scan_float_tail(chars, pos);
    pos = tail_pos;

    let raw: String = chars[start..pos].iter().collect();
    let tt = if is_float { TokenType::Float } else { TokenType::Int };
    let end_col = col + (pos - start);
    (Token { token_type: tt, value: raw, line, col, end_col }, pos)
}

/// Lexes a radix-prefixed integer literal (`0x…`/`0b…`/`0o…`), starting at
/// `chars[start] == '0'` with the prefix letter already identified. Returns
/// `None` when no digit of the radix follows the prefix (`0x`, `0b_`), so the
/// caller falls through to plain-decimal lexing and the bare prefix never
/// becomes a token. `literals::radix_and_digits` decodes all three prefixes.
fn lex_radix_number(
    chars: &[char],
    start: usize,
    line: usize,
    col: usize,
    radix: u32,
) -> Option<(Token, usize)> {
    let mut pos = start + 2;
    let mut digits = 0usize;
    while pos < chars.len() && (chars[pos].is_digit(radix) || chars[pos] == '_') {
        if chars[pos] != '_' {
            digits += 1;
        }
        pos += 1;
    }
    if digits == 0 {
        return None;
    }
    let raw: String = chars[start..pos].iter().collect();
    let end_col = col + (pos - start);
    Some((Token { token_type: TokenType::Int, value: raw, line, col, end_col }, pos))
}

/// Scans the float tail — an optional fraction (`.` + digit run) and an
/// optional exponent (`e`/`E` with sign) — returning the new position and
/// whether either was present (= the literal is a Float).
fn scan_float_tail(chars: &[char], mut pos: usize) -> (usize, bool) {
    let mut is_float = false;
    if pos < chars.len() && chars[pos] == '.' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit() {
        is_float = true;
        pos = scan_digit_run(chars, pos + 1);
    }
    // Scientific notation
    if pos < chars.len() && (chars[pos] == 'e' || chars[pos] == 'E') {
        is_float = true;
        pos += 1;
        if pos < chars.len() && (chars[pos] == '+' || chars[pos] == '-') { pos += 1; }
        while pos < chars.len() && chars[pos].is_ascii_digit() { pos += 1; }
    }
    (pos, is_float)
}

/// Scans a run of ASCII digits/underscores starting at `pos`, returning the
/// position just past the run. Shared by `lex_number`'s integer and
/// fractional-part scans (both allow `_` digit separators).
fn scan_digit_run(chars: &[char], mut pos: usize) -> usize {
    while pos < chars.len() && (chars[pos].is_ascii_digit() || chars[pos] == '_') { pos += 1; }
    pos
}

// ── Identifier / keyword lexing ─────────────────────────────────

fn lex_ident(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start;
    while pos < chars.len() && (chars[pos].is_ascii_alphanumeric() || chars[pos] == '_') { pos += 1; }

    let value: String = chars[start..pos].iter().collect();

    // Check if it's a keyword
    let token_type = keyword(&value).unwrap_or_else(|| {
        if value.chars().next().map_or(false, |c| c.is_uppercase()) { TokenType::TypeName }
        else { TokenType::Ident }
    });

    let end_col = col + (pos - start);
    (Token { token_type, value, line, col, end_col }, pos)
}

/// Lex a backtick-escaped identifier: `protocol`, `type`, etc.
/// The backticks are consumed but not included in the token value.
/// The result is always TokenType::Ident regardless of keyword status.
fn lex_backtick_ident(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start + 1; // skip opening backtick
    let ident_start = pos;
    while pos < chars.len() && (chars[pos].is_ascii_alphanumeric() || chars[pos] == '_') {
        pos += 1;
    }
    let value: String = chars[ident_start..pos].iter().collect();
    // Consume closing backtick if present
    if pos < chars.len() && chars[pos] == '`' {
        pos += 1;
    }
    let token_type = if value.chars().next().map_or(false, |c| c.is_uppercase()) {
        TokenType::TypeName
    } else {
        TokenType::Ident
    };
    let end_col = col + (pos - start);
    (Token { token_type, value, line, col, end_col }, pos)
}

/// The keyword table — data, not control flow: one row per spelling
/// (`ok`/`Ok` are two rows to the same token). Adding a keyword is adding a
/// row; `keyword` itself never changes.
const KEYWORDS: &[(&str, TokenType)] = &[
    ("module", TokenType::Module), ("import", TokenType::Import),
    ("type", TokenType::Type), ("protocol", TokenType::Protocol),
    ("for", TokenType::For), ("in", TokenType::In), ("fn", TokenType::Fn),
    ("let", TokenType::Let), ("var", TokenType::Var), ("mut", TokenType::Mut),
    ("if", TokenType::If), ("then", TokenType::Then),
    ("else", TokenType::Else), ("match", TokenType::Match),
    ("ok", TokenType::Ok), ("Ok", TokenType::Ok),
    ("err", TokenType::Err), ("Err", TokenType::Err),
    ("some", TokenType::Some), ("Some", TokenType::Some),
    ("none", TokenType::None), ("None", TokenType::None),
    ("todo", TokenType::Todo),
    ("true", TokenType::True), ("false", TokenType::False),
    ("not", TokenType::Not), ("and", TokenType::And),
    ("or", TokenType::Or), ("strict", TokenType::Strict),
    ("pub", TokenType::Pub), ("effect", TokenType::Effect),
    ("test", TokenType::Test),
    ("guard", TokenType::Guard), ("break", TokenType::Break),
    ("continue", TokenType::Continue), ("while", TokenType::While),
    ("local", TokenType::Local), ("mod", TokenType::Mod),
    ("fan", TokenType::Fan),
];

fn keyword(s: &str) -> Option<TokenType> {
    KEYWORDS.iter().find(|(k, _)| *k == s).map(|(_, t)| *t)
}

// ── Operator / delimiter lexing ─────────────────────────────────

/// The operator/delimiter table — longest pattern first, so a plain top-down
/// scan IS longest-match. Each row is (source pattern, token, token VALUE):
/// value differs from pattern only for aliases (`**` lexes as the power
/// operator `^`). Adding an operator is adding a row in length order.
const OPERATORS: &[(&str, TokenType, &str)] = &[
    // Three-char
    ("..=", TokenType::DotDotEq, "..="), ("...", TokenType::DotDotDot, "..."),
    ("..<", TokenType::DotDotLt, "..<"),
    // Two-char
    ("->", TokenType::Arrow, "->"), ("=>", TokenType::FatArrow, "=>"),
    ("==", TokenType::EqEq, "=="), ("!=", TokenType::BangEq, "!="),
    ("<=", TokenType::LtEq, "<="), (">>", TokenType::ComposeArrow, ">>"),
    (">=", TokenType::GtEq, ">="), ("++", TokenType::PlusPlus, "++"),
    ("|>", TokenType::PipeArrow, "|>"), ("&&", TokenType::AmpAmp, "&&"),
    ("||", TokenType::PipePipe, "||"), ("??", TokenType::QuestionQuestion, "??"),
    ("?.", TokenType::QuestionDot, "?."), ("..", TokenType::DotDot, ".."),
    ("**", TokenType::Caret, "^"), // ** is an alias for ^ (power)
    // Single-char
    ("(", TokenType::LParen, "("), (")", TokenType::RParen, ")"),
    ("{", TokenType::LBrace, "{"), ("}", TokenType::RBrace, "}"),
    ("[", TokenType::LBracket, "["), ("]", TokenType::RBracket, "]"),
    (",", TokenType::Comma, ","), (".", TokenType::Dot, "."),
    (":", TokenType::Colon, ":"), (";", TokenType::Semicolon, ";"),
    ("=", TokenType::Eq, "="), ("!", TokenType::Bang, "!"),
    ("<", TokenType::LAngle, "<"), (">", TokenType::RAngle, ">"),
    ("+", TokenType::Plus, "+"), ("-", TokenType::Minus, "-"),
    ("*", TokenType::Star, "*"), ("/", TokenType::Slash, "/"),
    ("%", TokenType::Percent, "%"), ("|", TokenType::Pipe, "|"),
    ("^", TokenType::Caret, "^"), ("?", TokenType::Question, "?"),
    ("_", TokenType::Underscore, "_"), ("@", TokenType::At, "@"),
];

fn lex_operator(chars: &[char], pos: usize, line: usize, col: usize) -> (Token, usize) {
    for (pat, tt, val) in OPERATORS {
        let len = pat.len(); // operator patterns are ASCII: bytes == chars
        if chars[pos..].len() >= len && pat.chars().zip(&chars[pos..]).all(|(p, &c)| p == c) {
            let tok = Token { token_type: *tt, value: (*val).to_string(), line, col, end_col: col + len };
            return (tok, len);
        }
    }
    // Unknown char: emit an Unknown token carrying the character. The old arm
    // returned an EOF-typed placeholder, which made the parser stop at the
    // first stray non-ASCII character and silently drop the rest of the file
    // with every gate green (#1308).
    (Token { token_type: TokenType::Unknown, value: chars[pos].to_string(), line, col, end_col: col + 1 }, 1)
}

fn peek(chars: &[char], pos: usize) -> Option<char> {
    chars.get(pos).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: a heredoc whose blank line holds multi-byte Unicode whitespace
    // (U+3000) used to panic in strip_heredoc_indent — the byte-offset dedent
    // sliced mid-codepoint. The lexer must never crash (almide-syntax contract).
    #[test]
    fn heredoc_multibyte_whitespace_line_does_not_panic() {
        let src = "let x = \"\"\"\n a\n\u{3000}\n  \"\"\"\n";
        let tokens = Lexer::tokenize(src);
        let s = tokens
            .iter()
            .find(|t| t.token_type == TokenType::String)
            .expect("heredoc should lex to a String token");
        // " a" dedents to "a"; the U+3000-only line is preserved (not split).
        assert_eq!(s.value, "a\n\u{3000}");
    }

    #[test]
    fn heredoc_ascii_dedent_still_works() {
        let src = "let x = \"\"\"\n    one\n    two\n    \"\"\"\n";
        let tokens = Lexer::tokenize(src);
        let s = tokens
            .iter()
            .find(|t| t.token_type == TokenType::String)
            .expect("heredoc should lex to a String token");
        assert_eq!(s.value, "one\ntwo");
    }

    fn lex_string_value(src: &str) -> String {
        Lexer::tokenize(src)
            .into_iter()
            .find(|t| t.token_type == TokenType::String)
            .expect("expected a String token")
            .value
    }

    // #746: numeric / Unicode escapes must decode, not pass through literally.
    #[test]
    fn numeric_and_unicode_escapes_decode() {
        // \xNN — two hex digits, codepoint 0..=0xFF (ESC control char).
        assert_eq!(lex_string_value("let x = \"\\x1b\"\n"), "\u{1b}");
        // \u{...} — ASCII 'A', a BMP codepoint, and an astral codepoint.
        assert_eq!(lex_string_value("let x = \"\\u{41}\"\n"), "A");
        assert_eq!(lex_string_value("let x = \"\\u{3042}\"\n"), "\u{3042}");
        assert_eq!(lex_string_value("let x = \"\\u{1f600}\"\n"), "\u{1f600}");
        // Interoperates with the basic escapes and literal text.
        assert_eq!(lex_string_value("let x = \"a\\x1bb\\n\"\n"), "a\u{1b}b\n");
        // Single-quote strings decode the same way.
        assert_eq!(lex_string_value("let x = '\\x1b'\n"), "\u{1b}");
        assert_eq!(lex_string_value("let x = '\\u{41}'\n"), "A");
    }

    fn lex_kinds_and_values(src: &str) -> Vec<(TokenType, String)> {
        Lexer::tokenize(src)
            .into_iter()
            .filter(|t| matches!(t.token_type, TokenType::Int | TokenType::Ident))
            .map(|t| (t.token_type, t.value))
            .collect()
    }

    // #873: all three radix prefixes lex as one Int token, in either case and
    // with `_` separators — the digits reach `literals::radix_and_digits` raw.
    #[test]
    fn radix_prefixed_int_literals_lex() {
        for src in ["let a = 0xFF\n", "let a = 0Xff\n", "let a = 0b1010\n",
                    "let a = 0B10_10\n", "let a = 0o17\n", "let a = 0O1_7\n"] {
            let toks = lex_kinds_and_values(src);
            assert_eq!(toks.len(), 2, "expected [a, literal] in {src:?}, got {toks:?}");
            assert_eq!(toks[1].0, TokenType::Int, "literal must be a single Int in {src:?}");
            assert!(toks[1].1.starts_with('0'), "raw value keeps the prefix: {toks:?}");
        }
    }

    // #873: a bare prefix (or prefix with only separators) must NOT lex as an
    // Int — `0` lexes alone and the rest starts an identifier, so the typo
    // fails to resolve instead of silently folding to 0.
    #[test]
    fn bare_radix_prefix_splits_into_zero_and_identifier() {
        for (src, ident) in [("let a = 0x\n", "x"), ("let a = 0b\n", "b"),
                             ("let a = 0o\n", "o"), ("let a = 0x_\n", "x_"),
                             ("let a = 0xg\n", "xg"), ("let a = 0b2\n", "b2")] {
            let toks = lex_kinds_and_values(src);
            assert_eq!(
                toks[1..],
                [(TokenType::Int, "0".to_string()), (TokenType::Ident, ident.to_string())],
                "in {src:?}"
            );
        }
    }

    // Malformed numeric escapes fall through to literal passthrough so existing
    // text (e.g. `\users`, `\xyz`, a surrogate) is preserved unchanged.
    #[test]
    fn malformed_numeric_escapes_pass_through() {
        assert_eq!(lex_string_value("let x = \"\\users\"\n"), "\\users");
        assert_eq!(lex_string_value("let x = \"\\xyz\"\n"), "\\xyz");
        assert_eq!(lex_string_value("let x = \"\\u{}\"\n"), "\\u{}");
        // U+D800 is a surrogate — not a valid scalar, so it stays literal.
        assert_eq!(lex_string_value("let x = \"\\u{d800}\"\n"), "\\u{d800}");
    }
}
