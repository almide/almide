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
    Ident, TypeName, IdentQ,
    // Keywords
    Module, Import, Type, Protocol, Impl, For, In, Fn, Let, Var,
    If, Then, Else, Match, Ok, Err, Some, None, Do, Todo,
    True, False, Not, And, Or,
    Strict, Pub, Effect, Test,
    Guard, Break, Continue, While, Local, Mod, Newtype, Fan,
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
    Plus, Minus, Star, StarStar, Slash, Percent,
    PlusPlus,  // ++
    Pipe,      // |
    PipeArrow, // |>
    Caret,     // ^
    AmpAmp,    // &&
    PipePipe,  // ||
    Underscore,
    DotDot,    // ..
    DotDotEq,  // ..=
    DotDotDot, // ...
    At,        // @
    // Whitespace / structure
    Comment, Newline, EOF,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: std::string::String,
    pub line: usize,
    pub col: usize,
}

// ── Lexer ───────────────────────────────────────────────────────

pub struct Lexer;

impl Lexer {
    pub fn tokenize(src: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = src.chars().collect();
        let mut pos = 0;
        let mut line = 1;
        let mut col = 1;

        while pos < chars.len() {
            let ch = chars[pos];

            // Skip whitespace (except newlines)
            if ch == ' ' || ch == '\t' || ch == '\r' {
                pos += 1; col += 1;
                continue;
            }

            // Newline
            if ch == '\n' {
                tokens.push(Token { token_type: TokenType::Newline, value: String::new(), line, col });
                pos += 1; line += 1; col = 1;
                continue;
            }

            // Line comment
            if ch == '/' && peek(&chars, pos + 1) == Some('/') {
                let (tok, new_pos) = lex_line_comment(&chars, pos, line, col);
                col += new_pos - pos;
                pos = new_pos;
                tokens.push(tok);
                continue;
            }

            // Block comment /* ... */ — nestable, fully skipped (not a token)
            if ch == '/' && peek(&chars, pos + 1) == Some('*') {
                let result = skip_block_comment(&chars, pos, line, col);
                pos = result.0; line = result.1; col = result.2;
                continue;
            }

            // Raw string literal: r"..." or r"""..."""
            if ch == 'r' && peek(&chars, pos + 1) == Some('"') {
                let (tok, new_pos, new_line, new_col) = lex_raw_string(&chars, pos, line, col);
                tokens.push(tok);
                pos = new_pos; line = new_line; col = new_col;
                continue;
            }

            // String literal (double or single quote)
            if ch == '"' {
                let (tok, new_pos, new_line, new_col) = lex_string(&chars, pos, line, col);
                tokens.push(tok);
                pos = new_pos; line = new_line; col = new_col;
                continue;
            }
            if ch == '\'' {
                let (tok, new_pos, new_line, new_col) = lex_single_quote_string(&chars, pos, line, col);
                tokens.push(tok);
                pos = new_pos; line = new_line; col = new_col;
                continue;
            }

            // Number
            if ch.is_ascii_digit() {
                let (tok, new_pos) = lex_number(&chars, pos, line, col);
                let len = new_pos - pos;
                tokens.push(tok);
                pos = new_pos; col += len;
                continue;
            }

            // Identifier or keyword
            if ch.is_ascii_alphabetic() || ch == '_' {
                let (tok, new_pos) = lex_ident(&chars, pos, line, col);
                let len = new_pos - pos;
                tokens.push(tok);
                pos = new_pos; col += len;
                continue;
            }

            // Operators and delimiters
            let (tok, len) = lex_operator(&chars, pos, line, col);
            tokens.push(tok);
            pos += len; col += len;
        }

        tokens.push(Token { token_type: TokenType::EOF, value: String::new(), line, col });
        tokens
    }
}

// ── Line comment lexing ─────────────────────────────────────────

fn lex_line_comment(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start;
    while pos < chars.len() && chars[pos] != '\n' { pos += 1; }
    let text: String = chars[start..pos].iter().collect();
    (Token { token_type: TokenType::Comment, value: text, line, col }, pos)
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
        let tok = Token { token_type: TokenType::String, value, line, col };
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
        let tok = Token { token_type: TokenType::String, value, line, col };
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

    while pos < chars.len() && chars[pos] != '"' {
        pos = lex_string_char(chars, pos, &mut value, &mut has_interpolation);
    }
    if pos < chars.len() { pos += 1; } // skip closing "

    let tt = if has_interpolation { TokenType::InterpolatedString } else { TokenType::String };
    let len = pos - start;
    (Token { token_type: tt, value, line, col }, pos, line, col + len)
}

/// Single-quote string: `'...'` — double quotes don't need escaping.
/// Supports interpolation `${expr}` and escape sequences (`\\`, `\'`, `\n`, `\t`).
fn lex_single_quote_string(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    let mut pos = start + 1; // skip opening '
    let mut value = String::new();
    let mut has_interpolation = false;

    while pos < chars.len() && chars[pos] != '\'' {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            let next = chars[pos + 1];
            match next {
                '\'' => { value.push('\''); pos += 2; }
                '\\' => { value.push('\\'); pos += 2; }
                'n' => { value.push('\n'); pos += 2; }
                't' => { value.push('\t'); pos += 2; }
                'r' => { value.push('\r'); pos += 2; }
                _ => { value.push(chars[pos]); pos += 1; }
            }
        } else if chars[pos] == '$' && pos + 1 < chars.len() && chars[pos + 1] == '{' {
            has_interpolation = true;
            pos = lex_interpolation(chars, pos, &mut value);
        } else {
            value.push(chars[pos]);
            pos += 1;
        }
    }
    if pos < chars.len() { pos += 1; } // skip closing '

    let tt = if has_interpolation { TokenType::InterpolatedString } else { TokenType::String };
    let len = pos - start;
    (Token { token_type: tt, value, line, col }, pos, line, col + len)
}

fn lex_heredoc(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize, usize, usize) {
    let mut pos = start + 3; // skip opening """
    let mut raw = String::new();
    let mut has_interpolation = false;
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
        pos = lex_string_char(chars, pos, &mut raw, &mut has_interpolation);
    }
    if pos + 2 < chars.len() {
        cur_col += 3; // closing """
        pos += 3;
    }

    let value = strip_heredoc_indent(&raw);
    let tt = if has_interpolation { TokenType::InterpolatedString } else { TokenType::String };
    (Token { token_type: tt, value, line, col }, pos, cur_line, cur_col)
}

/// Process one character (or escape / interpolation) inside a string body.
/// Returns the new position after consuming the character(s).
fn lex_string_char(chars: &[char], pos: usize, buf: &mut String, has_interpolation: &mut bool) -> usize {
    if chars[pos] == '\\' && pos + 1 < chars.len() {
        lex_escape(chars, pos, buf)
    } else if chars[pos] == '$' && pos + 1 < chars.len() && chars[pos + 1] == '{' {
        *has_interpolation = true;
        lex_interpolation(chars, pos, buf)
    } else {
        buf.push(chars[pos]);
        pos + 1
    }
}

/// Process a backslash escape sequence. Returns the new position.
fn lex_escape(chars: &[char], pos: usize, buf: &mut String) -> usize {
    match chars[pos + 1] {
        'n' => { buf.push('\n'); pos + 2 }
        't' => { buf.push('\t'); pos + 2 }
        'r' => { buf.push('\r'); pos + 2 }
        '\\' => { buf.push('\\'); pos + 2 }
        '"' => { buf.push('"'); pos + 2 }
        '$' => { buf.push('$'); pos + 2 }
        other => { buf.push('\\'); buf.push(other); pos + 2 }
    }
}

/// Process an interpolation `${...}` block. Returns the new position.
fn lex_interpolation(chars: &[char], start: usize, buf: &mut String) -> usize {
    buf.push('$');
    buf.push('{');
    let mut pos = start + 2;
    let mut depth = 1;
    while pos < chars.len() && depth > 0 {
        if chars[pos] == '{' { depth += 1; }
        if chars[pos] == '}' { depth -= 1; }
        if depth > 0 { buf.push(chars[pos]); }
        pos += 1;
    }
    buf.push('}');
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
        .map(|l| if l.len() >= indent { &l[indent..] } else { "" })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Number lexing ───────────────────────────────────────────────

fn lex_number(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start;
    let mut is_float = false;

    // Hex: 0x...
    if chars[pos] == '0' && pos + 1 < chars.len() && (chars[pos + 1] == 'x' || chars[pos + 1] == 'X') {
        pos += 2;
        while pos < chars.len() && (chars[pos].is_ascii_hexdigit() || chars[pos] == '_') { pos += 1; }
        let raw: String = chars[start..pos].iter().collect();
        return (Token { token_type: TokenType::Int, value: raw, line, col }, pos);
    }

    while pos < chars.len() && (chars[pos].is_ascii_digit() || chars[pos] == '_') { pos += 1; }

    if pos < chars.len() && chars[pos] == '.' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit() {
        is_float = true;
        pos += 1;
        while pos < chars.len() && (chars[pos].is_ascii_digit() || chars[pos] == '_') { pos += 1; }
    }

    // Scientific notation
    if pos < chars.len() && (chars[pos] == 'e' || chars[pos] == 'E') {
        is_float = true;
        pos += 1;
        if pos < chars.len() && (chars[pos] == '+' || chars[pos] == '-') { pos += 1; }
        while pos < chars.len() && chars[pos].is_ascii_digit() { pos += 1; }
    }

    let raw: String = chars[start..pos].iter().collect();
    let tt = if is_float { TokenType::Float } else { TokenType::Int };
    (Token { token_type: tt, value: raw, line, col }, pos)
}

// ── Identifier / keyword lexing ─────────────────────────────────

fn lex_ident(chars: &[char], start: usize, line: usize, col: usize) -> (Token, usize) {
    let mut pos = start;
    while pos < chars.len() && (chars[pos].is_ascii_alphanumeric() || chars[pos] == '_') { pos += 1; }

    // Trailing ? for query methods (e.g., is_empty?)
    if pos < chars.len() && chars[pos] == '?' {
        pos += 1;
    }

    let value: String = chars[start..pos].iter().collect();

    // Check if it's a keyword
    let token_type = keyword(&value).unwrap_or_else(|| {
        if value.ends_with('?') { TokenType::IdentQ }
        else if value.chars().next().map_or(false, |c| c.is_uppercase()) { TokenType::TypeName }
        else { TokenType::Ident }
    });

    (Token { token_type, value, line, col }, pos)
}

fn keyword(s: &str) -> Option<TokenType> {
    match s {
        "module" => Some(TokenType::Module), "import" => Some(TokenType::Import),
        "type" => Some(TokenType::Type), "protocol" => Some(TokenType::Protocol),
        "impl" => Some(TokenType::Impl), "for" => Some(TokenType::For),
        "in" => Some(TokenType::In), "fn" => Some(TokenType::Fn),
        "let" => Some(TokenType::Let), "var" => Some(TokenType::Var),
        "if" => Some(TokenType::If), "then" => Some(TokenType::Then),
        "else" => Some(TokenType::Else), "match" => Some(TokenType::Match),
        "ok" => Some(TokenType::Ok), "err" => Some(TokenType::Err),
        "some" => Some(TokenType::Some), "none" => Some(TokenType::None),
        "do" => Some(TokenType::Do),
        "todo" => Some(TokenType::Todo),
        "true" => Some(TokenType::True), "false" => Some(TokenType::False),
        "not" => Some(TokenType::Not), "and" => Some(TokenType::And),
        "or" => Some(TokenType::Or), "strict" => Some(TokenType::Strict),
        "pub" => Some(TokenType::Pub), "effect" => Some(TokenType::Effect),
        "test" => Some(TokenType::Test),
        "guard" => Some(TokenType::Guard), "break" => Some(TokenType::Break),
        "continue" => Some(TokenType::Continue), "while" => Some(TokenType::While),
        "local" => Some(TokenType::Local), "mod" => Some(TokenType::Mod),
        "newtype" => Some(TokenType::Newtype),
        "fan" => Some(TokenType::Fan),
        _ => None,
    }
}

// ── Operator / delimiter lexing ─────────────────────────────────

fn lex_operator(chars: &[char], pos: usize, line: usize, col: usize) -> (Token, usize) {
    let ch = chars[pos];
    let next = peek(chars, pos + 1);
    let next2 = peek(chars, pos + 2);

    let (tt, val, len) = match (ch, next, next2) {
        // Three-char
        ('.', Some('.'), Some('=')) => (TokenType::DotDotEq, "..=", 3),
        ('.', Some('.'), Some('.')) => (TokenType::DotDotDot, "...", 3),
        // Two-char
        ('-', Some('>'), _) => (TokenType::Arrow, "->", 2),
        ('=', Some('>'), _) => (TokenType::FatArrow, "=>", 2),
        ('=', Some('='), _) => (TokenType::EqEq, "==", 2),
        ('!', Some('='), _) => (TokenType::BangEq, "!=", 2),
        ('<', Some('='), _) => (TokenType::LtEq, "<=", 2),
        ('>', Some('='), _) => (TokenType::GtEq, ">=", 2),
        ('+', Some('+'), _) => (TokenType::PlusPlus, "++", 2),
        ('|', Some('>'), _) => (TokenType::PipeArrow, "|>", 2),
        ('&', Some('&'), _) => (TokenType::AmpAmp, "&&", 2),
        ('|', Some('|'), _) => (TokenType::PipePipe, "||", 2),
        ('.', Some('.'), _) => (TokenType::DotDot, "..", 2),
        // Single-char
        ('(', _, _) => (TokenType::LParen, "(", 1),
        (')', _, _) => (TokenType::RParen, ")", 1),
        ('{', _, _) => (TokenType::LBrace, "{", 1),
        ('}', _, _) => (TokenType::RBrace, "}", 1),
        ('[', _, _) => (TokenType::LBracket, "[", 1),
        (']', _, _) => (TokenType::RBracket, "]", 1),
        (',', _, _) => (TokenType::Comma, ",", 1),
        ('.', _, _) => (TokenType::Dot, ".", 1),
        (':', _, _) => (TokenType::Colon, ":", 1),
        (';', _, _) => (TokenType::Semicolon, ";", 1),
        ('=', _, _) => (TokenType::Eq, "=", 1),
        ('!', _, _) => (TokenType::Bang, "!", 1),
        ('<', _, _) => (TokenType::LAngle, "<", 1),
        ('>', _, _) => (TokenType::RAngle, ">", 1),
        ('+', _, _) => (TokenType::Plus, "+", 1),
        ('-', _, _) => (TokenType::Minus, "-", 1),
        ('*', Some('*'), _) => (TokenType::StarStar, "**", 2),
        ('*', _, _) => (TokenType::Star, "*", 1),
        ('/', _, _) => (TokenType::Slash, "/", 1),
        ('%', _, _) => (TokenType::Percent, "%", 1),
        ('|', _, _) => (TokenType::Pipe, "|", 1),
        ('^', _, _) => (TokenType::Caret, "^", 1),
        ('_', _, _) => (TokenType::Underscore, "_", 1),
        ('@', _, _) => (TokenType::At, "@", 1),
        _ => (TokenType::EOF, "", 1), // skip unknown char
    };

    (Token { token_type: tt, value: val.to_string(), line, col }, len)
}

fn peek(chars: &[char], pos: usize) -> Option<char> {
    chars.get(pos).copied()
}
