use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Literals
    Int,
    Float,
    String,
    InterpolatedString,

    // Identifiers & Names
    Ident,
    TypeName,
    IdentQ,

    // Keywords
    Module,
    Import,
    Type,
    Trait,
    Impl,
    For,
    In,
    Fn,
    Let,
    Var,
    If,
    Then,
    Else,
    Match,
    Ok,
    Err,
    Some,
    None,
    Try,
    Do,
    Todo,
    Unsafe,
    True,
    False,
    Not,
    And,
    Or,
    Strict,
    Pub,
    Effect,
    Deriving,
    Test,
    Async,
    Await,
    Guard,
    Newtype,

    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LAngle,
    RAngle,
    Comma,
    Dot,
    Colon,
    Semicolon,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    Bang,
    BangEq,
    LtEq,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    Pipe,
    PipeArrow,
    Caret,
    Underscore,
    DotDot,
    DotDotEq,
    DotDotDot,

    // Special
    Comment,
    Newline,
    EOF,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub value: std::string::String,
    pub line: usize,
    pub col: usize,
}

fn build_keyword_map() -> HashMap<&'static str, TokenType> {
    let mut m = HashMap::new();
    m.insert("module", TokenType::Module);
    m.insert("import", TokenType::Import);
    m.insert("type", TokenType::Type);
    m.insert("trait", TokenType::Trait);
    m.insert("impl", TokenType::Impl);
    m.insert("for", TokenType::For);
    m.insert("in", TokenType::In);
    m.insert("fn", TokenType::Fn);
    m.insert("let", TokenType::Let);
    m.insert("var", TokenType::Var);
    m.insert("if", TokenType::If);
    m.insert("then", TokenType::Then);
    m.insert("else", TokenType::Else);
    m.insert("match", TokenType::Match);
    m.insert("ok", TokenType::Ok);
    m.insert("Ok", TokenType::Ok);
    m.insert("err", TokenType::Err);
    m.insert("Err", TokenType::Err);
    m.insert("some", TokenType::Some);
    m.insert("Some", TokenType::Some);
    m.insert("none", TokenType::None);
    m.insert("None", TokenType::None);
    m.insert("try", TokenType::Try);
    m.insert("do", TokenType::Do);
    m.insert("todo", TokenType::Todo);
    m.insert("unsafe", TokenType::Unsafe);
    m.insert("true", TokenType::True);
    m.insert("false", TokenType::False);
    m.insert("not", TokenType::Not);
    m.insert("and", TokenType::And);
    m.insert("or", TokenType::Or);
    m.insert("strict", TokenType::Strict);
    m.insert("pub", TokenType::Pub);
    m.insert("effect", TokenType::Effect);
    m.insert("deriving", TokenType::Deriving);
    m.insert("test", TokenType::Test);
    m.insert("async", TokenType::Async);
    m.insert("await", TokenType::Await);
    m.insert("guard", TokenType::Guard);
    m.insert("newtype", TokenType::Newtype);
    m
}

fn is_continuation_token(tt: &TokenType) -> bool {
    matches!(
        tt,
        TokenType::Dot
            | TokenType::Comma
            | TokenType::LParen
            | TokenType::LBrace
            | TokenType::LBracket
            | TokenType::Plus
            | TokenType::Minus
            | TokenType::Star
            | TokenType::Slash
            | TokenType::Percent
            | TokenType::PlusPlus
            | TokenType::Pipe
            | TokenType::PipeArrow
            | TokenType::Arrow
            | TokenType::FatArrow
            | TokenType::Eq
            | TokenType::EqEq
            | TokenType::Bang
            | TokenType::BangEq
            | TokenType::LtEq
            | TokenType::GtEq
            | TokenType::LAngle
            | TokenType::RAngle
            | TokenType::And
            | TokenType::Or
            | TokenType::Not
            | TokenType::Colon
            | TokenType::If
            | TokenType::Then
            | TokenType::Else
            | TokenType::Match
            | TokenType::Try
            | TokenType::Await
            | TokenType::Do
            | TokenType::Guard
            | TokenType::DotDot
            | TokenType::DotDotEq
    )
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    tokens: Vec<Token>,
    keywords: HashMap<&'static str, TokenType>,
}

impl Lexer {
    pub fn tokenize(src: &str) -> Vec<Token> {
        let mut lexer = Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            tokens: Vec::new(),
            keywords: build_keyword_map(),
        };
        lexer.run();
        lexer.tokens
    }

    fn run(&mut self) {
        while self.pos < self.chars.len() {
            self.skip_spaces_and_comments();
            if self.pos >= self.chars.len() {
                break;
            }

            let ch = self.chars[self.pos];

            // Newline
            if ch == '\n' {
                self.add_newline();
                self.advance();
                continue;
            }

            // String literal
            if ch == '"' {
                self.read_string();
                continue;
            }

            // Number
            if ch.is_ascii_digit() {
                self.read_number();
                continue;
            }

            // Raw string literal r"..."
            if ch == 'r' && self.peek(1) == '"' {
                self.read_raw_string();
                continue;
            }

            // Identifier or keyword (lowercase or _)
            if ch.is_ascii_lowercase() || ch == '_' {
                // special case: _ alone is Underscore
                if ch == '_' && !self.is_alpha_num(self.peek(1)) {
                    self.add_token(TokenType::Underscore, "_".to_string());
                    self.advance();
                    continue;
                }
                self.read_identifier();
                continue;
            }

            // Type name (uppercase)
            if ch.is_ascii_uppercase() {
                self.read_type_name();
                continue;
            }

            // Symbols
            if self.read_symbol() {
                continue;
            }

            // Unknown character - skip
            self.advance();
        }

        self.add_token(TokenType::EOF, String::new());
    }

    fn skip_spaces_and_comments(&mut self) {
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else if ch == '/' && self.peek(1) == '/' {
                // Line comment — emit as Comment token
                let start_line = self.line;
                let start_col = self.col;
                let mut text = String::new();
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    text.push(self.chars[self.pos]);
                    self.advance();
                }
                self.tokens.push(Token {
                    token_type: TokenType::Comment,
                    value: text,
                    line: start_line,
                    col: start_col,
                });
            } else {
                break;
            }
        }
    }

    fn add_newline(&mut self) {
        // Skip newline if previous token is a continuation token
        if let Some(last) = self.tokens.last() {
            if is_continuation_token(&last.token_type) {
                self.line += 1;
                self.col = 1;
                return;
            }
            // Skip duplicate newlines
            if last.token_type == TokenType::Newline {
                self.line += 1;
                self.col = 1;
                return;
            }
        } else {
            // Skip newline at start (no tokens yet)
            self.line += 1;
            self.col = 1;
            return;
        }

        // Skip newline if next non-whitespace starts a continuation (. or |>)
        if self.peek_next_non_whitespace() {
            self.line += 1;
            self.col = 1;
            return;
        }

        self.add_token(TokenType::Newline, "\\n".to_string());
        self.line += 1;
        self.col = 1;
    }

    fn peek_next_non_whitespace(&self) -> bool {
        let mut i = self.pos + 1; // skip past current \n
        while i < self.chars.len()
            && (self.chars[i] == ' '
                || self.chars[i] == '\t'
                || self.chars[i] == '\r'
                || self.chars[i] == '\n')
        {
            i += 1;
        }
        if i >= self.chars.len() {
            return false;
        }
        // Leading dot (method chain)
        if self.chars[i] == '.' {
            return true;
        }
        // Leading |> (pipe)
        if self.chars[i] == '|' && i + 1 < self.chars.len() && self.chars[i + 1] == '>' {
            return true;
        }
        false
    }

    fn read_string(&mut self) {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // skip opening "
        let mut value = String::new();
        let mut has_interpolation = false;

        while self.pos < self.chars.len() && self.chars[self.pos] != '"' {
            if self.chars[self.pos] == '$' && self.peek(1) == '{' {
                has_interpolation = true;
            }
            if self.chars[self.pos] == '\\' {
                self.advance();
                if self.pos < self.chars.len() {
                    let esc = self.chars[self.pos];
                    match esc {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        '\\' => value.push('\\'),
                        '"' => value.push('"'),
                        '$' => value.push('$'),
                        other => value.push(other),
                    }
                    self.advance();
                }
            } else {
                value.push(self.chars[self.pos]);
                self.advance();
            }
        }
        if self.pos < self.chars.len() {
            self.advance(); // skip closing "
        }

        let token_type = if has_interpolation {
            TokenType::InterpolatedString
        } else {
            TokenType::String
        };
        self.tokens.push(Token {
            token_type,
            value,
            line: start_line,
            col: start_col,
        });
    }

    fn read_raw_string(&mut self) {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // skip 'r'
        self.advance(); // skip '"'
        let mut value = String::new();
        while self.pos < self.chars.len() && self.chars[self.pos] != '"' {
            value.push(self.chars[self.pos]);
            self.advance();
        }
        if self.pos < self.chars.len() {
            self.advance(); // skip closing "
        }
        self.tokens.push(Token {
            token_type: TokenType::String,
            value,
            line: start_line,
            col: start_col,
        });
    }

    fn read_number(&mut self) {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();
        let mut is_float = false;

        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
            value.push(self.chars[self.pos]);
            self.advance();
        }

        if self.pos < self.chars.len()
            && self.chars[self.pos] == '.'
            && self.peek(1).is_ascii_digit()
        {
            is_float = true;
            value.push('.');
            self.advance();
            while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
                value.push(self.chars[self.pos]);
                self.advance();
            }
        }

        let token_type = if is_float {
            TokenType::Float
        } else {
            TokenType::Int
        };
        self.tokens.push(Token {
            token_type,
            value,
            line: start_line,
            col: start_col,
        });
    }

    fn read_identifier(&mut self) {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();

        while self.pos < self.chars.len() && self.is_alpha_num(self.chars[self.pos]) {
            value.push(self.chars[self.pos]);
            self.advance();
        }

        // Check for ? suffix (Bool predicates)
        if self.pos < self.chars.len() && self.chars[self.pos] == '?' {
            value.push('?');
            self.advance();
            self.tokens.push(Token {
                token_type: TokenType::IdentQ,
                value,
                line: start_line,
                col: start_col,
            });
            return;
        }

        // Check keywords
        let token_type = if let Some(kw) = self.keywords.get(value.as_str()) {
            kw.clone()
        } else {
            TokenType::Ident
        };
        self.tokens.push(Token {
            token_type,
            value,
            line: start_line,
            col: start_col,
        });
    }

    fn read_type_name(&mut self) {
        let start_line = self.line;
        let start_col = self.col;
        let mut value = String::new();

        while self.pos < self.chars.len() && self.is_alpha_num(self.chars[self.pos]) {
            value.push(self.chars[self.pos]);
            self.advance();
        }

        let token_type = self.keywords.get(value.as_str()).cloned().unwrap_or(TokenType::TypeName);

        self.tokens.push(Token {
            token_type,
            value,
            line: start_line,
            col: start_col,
        });
    }

    fn read_symbol(&mut self) -> bool {
        let start_line = self.line;
        let start_col = self.col;

        let c = self.chars[self.pos];
        let c2 = self.peek(1);
        let c3 = self.peek(2);

        // Three-char: ... and ..=
        if c == '.' && c2 == '.' && c3 == '.' {
            self.add_token(TokenType::DotDotDot, "...".to_string());
            self.advance();
            self.advance();
            self.advance();
            return true;
        }
        if c == '.' && c2 == '.' && c3 == '=' {
            self.add_token(TokenType::DotDotEq, "..=".to_string());
            self.advance();
            self.advance();
            self.advance();
            return true;
        }
        // Two-char: ..
        if c == '.' && c2 == '.' {
            self.add_token(TokenType::DotDot, "..".to_string());
            self.advance();
            self.advance();
            return true;
        }

        // Two-char tokens
        let two: String = [c, c2].iter().collect();
        let two_char_type = match two.as_str() {
            "->" => Some(TokenType::Arrow),
            "=>" => Some(TokenType::FatArrow),
            "==" => Some(TokenType::EqEq),
            "!=" => Some(TokenType::BangEq),
            "<=" => Some(TokenType::LtEq),
            ">=" => Some(TokenType::GtEq),
            "++" => Some(TokenType::PlusPlus),
            "|>" => Some(TokenType::PipeArrow),
            _ => Option::None,
        };
        if let Some(tt) = two_char_type {
            self.tokens.push(Token {
                token_type: tt,
                value: two,
                line: start_line,
                col: start_col,
            });
            self.advance();
            self.advance();
            return true;
        }

        // Single-char tokens
        let one_char_type = match c {
            '(' => Some(TokenType::LParen),
            ')' => Some(TokenType::RParen),
            '{' => Some(TokenType::LBrace),
            '}' => Some(TokenType::RBrace),
            '[' => Some(TokenType::LBracket),
            ']' => Some(TokenType::RBracket),
            '<' => Some(TokenType::LAngle),
            '>' => Some(TokenType::RAngle),
            ',' => Some(TokenType::Comma),
            '.' => Some(TokenType::Dot),
            ':' => Some(TokenType::Colon),
            ';' => Some(TokenType::Semicolon),
            '=' => Some(TokenType::Eq),
            '+' => Some(TokenType::Plus),
            '-' => Some(TokenType::Minus),
            '*' => Some(TokenType::Star),
            '/' => Some(TokenType::Slash),
            '%' => Some(TokenType::Percent),
            '|' => Some(TokenType::Pipe),
            '^' => Some(TokenType::Caret),
            '!' => Some(TokenType::Bang),
            '_' => Some(TokenType::Underscore),
            _ => Option::None,
        };
        if let Some(tt) = one_char_type {
            self.tokens.push(Token {
                token_type: tt,
                value: c.to_string(),
                line: start_line,
                col: start_col,
            });
            self.advance();
            return true;
        }

        false
    }

    fn add_token(&mut self, token_type: TokenType, value: String) {
        self.tokens.push(Token {
            token_type,
            value,
            line: self.line,
            col: self.col,
        });
    }

    fn advance(&mut self) {
        self.pos += 1;
        self.col += 1;
    }

    fn peek(&self, offset: usize) -> char {
        let idx = self.pos + offset;
        if idx < self.chars.len() {
            self.chars[idx]
        } else {
            '\0'
        }
    }

    fn is_alpha_num(&self, ch: char) -> bool {
        ch.is_ascii_lowercase() || ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'
    }
}
