// Token types for the LLM-native language

export enum TokenType {
  // Literals
  Int = "Int",
  Float = "Float",
  String = "String",
  InterpolatedString = "InterpolatedString",

  // Identifiers & Names
  Ident = "Ident",        // lowercase identifiers
  TypeName = "TypeName",  // uppercase identifiers
  IdentQ = "IdentQ",      // name?

  // Keywords
  Module = "module",
  Import = "import",
  Type = "type",
  Trait = "trait",
  Impl = "impl",
  For = "for",
  Fn = "fn",
  Let = "let",
  Var = "var",
  If = "if",
  Then = "then",
  Else = "else",
  Match = "match",
  Ok = "ok",
  Err = "err",
  Some = "some",
  None = "none",
  Try = "try",
  Do = "do",
  Todo = "todo",
  Unsafe = "unsafe",
  True = "true",
  False = "false",
  Not = "not",
  And = "and",
  Or = "or",
  Strict = "strict",
  Pub = "pub",
  Effect = "effect",
  Deriving = "deriving",
  Test = "test",
  Async = "async",
  Await = "await",
  Guard = "guard",
  Newtype = "newtype",

  // Symbols
  LParen = "(",
  RParen = ")",
  LBrace = "{",
  RBrace = "}",
  LBracket = "[",
  RBracket = "]",
  LAngle = "<",
  RAngle = ">",
  Comma = ",",
  Dot = ".",
  Colon = ":",
  Semicolon = ";",
  Arrow = "->",
  FatArrow = "=>",
  Eq = "=",
  EqEq = "==",
  BangEq = "!=",
  LtEq = "<=",
  GtEq = ">=",
  Plus = "+",
  Minus = "-",
  Star = "*",
  Slash = "/",
  Percent = "%",
  PlusPlus = "++",
  Pipe = "|",
  PipeArrow = "|>",
  Caret = "^",
  Underscore = "_",
  DotDotDot = "...",

  // Special
  Newline = "Newline",
  EOF = "EOF",
}

const KEYWORDS: Record<string, TokenType> = {
  module: TokenType.Module,
  import: TokenType.Import,
  type: TokenType.Type,
  trait: TokenType.Trait,
  impl: TokenType.Impl,
  for: TokenType.For,
  fn: TokenType.Fn,
  let: TokenType.Let,
  var: TokenType.Var,
  if: TokenType.If,
  then: TokenType.Then,
  else: TokenType.Else,
  match: TokenType.Match,
  ok: TokenType.Ok,
  err: TokenType.Err,
  some: TokenType.Some,
  none: TokenType.None,
  try: TokenType.Try,
  do: TokenType.Do,
  todo: TokenType.Todo,
  unsafe: TokenType.Unsafe,
  true: TokenType.True,
  false: TokenType.False,
  not: TokenType.Not,
  and: TokenType.And,
  or: TokenType.Or,
  strict: TokenType.Strict,
  pub: TokenType.Pub,
  effect: TokenType.Effect,
  deriving: TokenType.Deriving,
  test: TokenType.Test,
  async: TokenType.Async,
  await: TokenType.Await,
  guard: TokenType.Guard,
  newtype: TokenType.Newtype,
};

export interface Token {
  type: TokenType;
  value: string;
  line: number;
  col: number;
}

// Tokens that suppress a following newline (line continuation)
const CONTINUATION_TOKENS = new Set<TokenType>([
  TokenType.Dot,
  TokenType.Comma,
  TokenType.LParen,
  TokenType.LBrace,
  TokenType.LBracket,
  TokenType.Plus,
  TokenType.Minus,
  TokenType.Star,
  TokenType.Slash,
  TokenType.Percent,
  TokenType.PlusPlus,
  TokenType.Pipe,
  TokenType.PipeArrow,
  TokenType.Arrow,
  TokenType.FatArrow,
  TokenType.Eq,
  TokenType.EqEq,
  TokenType.BangEq,
  TokenType.LtEq,
  TokenType.GtEq,
  TokenType.LAngle,
  TokenType.RAngle,
  TokenType.And,
  TokenType.Or,
  TokenType.Not,
  TokenType.Colon,
  // Keywords that expect continuation
  TokenType.If,
  TokenType.Then,
  TokenType.Else,
  TokenType.Match,
  TokenType.Try,
  TokenType.Await,
  TokenType.Do,
  TokenType.Guard,
]);

export class Lexer {
  private src: string;
  private pos: number = 0;
  private line: number = 1;
  private col: number = 1;
  private tokens: Token[] = [];

  constructor(src: string) {
    this.src = src;
  }

  tokenize(): Token[] {
    while (this.pos < this.src.length) {
      this.skipSpacesAndComments();
      if (this.pos >= this.src.length) break;

      const ch = this.src[this.pos];

      // Newline
      if (ch === "\n") {
        this.addNewline();
        this.advance();
        continue;
      }

      // String literal
      if (ch === '"') {
        this.readString();
        continue;
      }

      // Number
      if (this.isDigit(ch)) {
        this.readNumber();
        continue;
      }

      // Identifier or keyword (lowercase)
      if (this.isLowerAlpha(ch) || ch === "_") {
        // special case: _ alone is Underscore
        if (ch === "_" && !this.isAlphaNum(this.peek(1))) {
          this.addToken(TokenType.Underscore, "_");
          this.advance();
          continue;
        }
        this.readIdentifier();
        continue;
      }

      // Type name (uppercase)
      if (this.isUpperAlpha(ch)) {
        this.readTypeName();
        continue;
      }

      // Symbols
      if (this.readSymbol()) continue;

      // Unknown character - skip
      this.advance();
    }

    this.addToken(TokenType.EOF, "");
    return this.tokens;
  }

  private skipSpacesAndComments(): void {
    while (this.pos < this.src.length) {
      const ch = this.src[this.pos];
      if (ch === " " || ch === "\t" || ch === "\r") {
        this.advance();
      } else if (ch === "/" && this.peek(1) === "/") {
        // Line comment
        while (this.pos < this.src.length && this.src[this.pos] !== "\n") {
          this.advance();
        }
      } else {
        break;
      }
    }
  }

  private addNewline(): void {
    // Skip newline if previous token is a continuation token
    const last = this.tokens.length > 0 ? this.tokens[this.tokens.length - 1] : null;
    if (last && CONTINUATION_TOKENS.has(last.type)) {
      this.line++;
      this.col = 1;
      return;
    }
    // Skip duplicate newlines
    if (last && last.type === TokenType.Newline) {
      this.line++;
      this.col = 1;
      return;
    }
    // Skip newline at start
    if (!last) {
      this.line++;
      this.col = 1;
      return;
    }
    // Skip newline if next non-whitespace starts a continuation (. or |>)
    if (this.peekNextNonWhitespace()) {
      this.line++;
      this.col = 1;
      return;
    }
    this.addToken(TokenType.Newline, "\\n");
    this.line++;
    this.col = 1;
  }

  private peekNextNonWhitespace(): boolean {
    let i = this.pos + 1; // skip past current \n
    while (i < this.src.length && (this.src[i] === " " || this.src[i] === "\t" || this.src[i] === "\r" || this.src[i] === "\n")) {
      i++;
    }
    if (i >= this.src.length) return false;
    // Leading dot (method chain)
    if (this.src[i] === ".") return true;
    // Leading |> (pipe)
    if (this.src[i] === "|" && i + 1 < this.src.length && this.src[i + 1] === ">") return true;
    return false;
  }

  private readString(): void {
    const startLine = this.line;
    const startCol = this.col;
    this.advance(); // skip opening "
    let value = "";
    let hasInterpolation = false;

    while (this.pos < this.src.length && this.src[this.pos] !== '"') {
      if (this.src[this.pos] === "$" && this.peek(1) === "{") {
        hasInterpolation = true;
      }
      if (this.src[this.pos] === "\\") {
        this.advance();
        if (this.pos < this.src.length) {
          const esc = this.src[this.pos];
          switch (esc) {
            case "n": value += "\n"; break;
            case "t": value += "\t"; break;
            case "\\": value += "\\"; break;
            case '"': value += '"'; break;
            case "$": value += "$"; break;
            default: value += esc;
          }
          this.advance();
        }
      } else {
        value += this.src[this.pos];
        this.advance();
      }
    }
    if (this.pos < this.src.length) this.advance(); // skip closing "

    this.tokens.push({
      type: hasInterpolation ? TokenType.InterpolatedString : TokenType.String,
      value,
      line: startLine,
      col: startCol,
    });
  }

  private readNumber(): void {
    const startLine = this.line;
    const startCol = this.col;
    let value = "";
    let isFloat = false;

    while (this.pos < this.src.length && this.isDigit(this.src[this.pos])) {
      value += this.src[this.pos];
      this.advance();
    }

    if (this.pos < this.src.length && this.src[this.pos] === "." && this.isDigit(this.peek(1) ?? "")) {
      isFloat = true;
      value += ".";
      this.advance();
      while (this.pos < this.src.length && this.isDigit(this.src[this.pos])) {
        value += this.src[this.pos];
        this.advance();
      }
    }

    this.tokens.push({
      type: isFloat ? TokenType.Float : TokenType.Int,
      value,
      line: startLine,
      col: startCol,
    });
  }

  private readIdentifier(): void {
    const startLine = this.line;
    const startCol = this.col;
    let value = "";

    while (this.pos < this.src.length && this.isAlphaNum(this.src[this.pos])) {
      value += this.src[this.pos];
      this.advance();
    }

    // Check for ? suffix (Bool predicates)
    if (this.pos < this.src.length && this.src[this.pos] === "?") {
      value += "?";
      this.advance();
      this.tokens.push({ type: TokenType.IdentQ, value, line: startLine, col: startCol });
      return;
    }

    // Check keywords
    const kw = KEYWORDS[value];
    if (kw) {
      this.tokens.push({ type: kw, value, line: startLine, col: startCol });
    } else {
      this.tokens.push({ type: TokenType.Ident, value, line: startLine, col: startCol });
    }
  }

  private readTypeName(): void {
    const startLine = this.line;
    const startCol = this.col;
    let value = "";

    while (this.pos < this.src.length && this.isAlphaNum(this.src[this.pos])) {
      value += this.src[this.pos];
      this.advance();
    }

    this.tokens.push({ type: TokenType.TypeName, value, line: startLine, col: startCol });
  }

  private readSymbol(): boolean {
    const startLine = this.line;
    const startCol = this.col;

    const c = this.src[this.pos];
    const c2 = this.pos + 1 < this.src.length ? this.src[this.pos + 1] : "";
    const c3 = this.pos + 2 < this.src.length ? this.src[this.pos + 2] : "";

    // Three-char
    if (c === "." && c2 === "." && c3 === ".") {
      this.addToken(TokenType.DotDotDot, "...");
      this.advance(); this.advance(); this.advance();
      return true;
    }

    // Two-char
    const two = c + c2;
    const twoCharTokens: Record<string, TokenType> = {
      "->": TokenType.Arrow,
      "=>": TokenType.FatArrow,
      "==": TokenType.EqEq,
      "!=": TokenType.BangEq,
      "<=": TokenType.LtEq,
      ">=": TokenType.GtEq,
      "++": TokenType.PlusPlus,
      "|>": TokenType.PipeArrow,
    };
    if (twoCharTokens[two]) {
      this.tokens.push({ type: twoCharTokens[two], value: two, line: startLine, col: startCol });
      this.advance(); this.advance();
      return true;
    }

    // Single-char
    const oneCharTokens: Record<string, TokenType> = {
      "(": TokenType.LParen,
      ")": TokenType.RParen,
      "{": TokenType.LBrace,
      "}": TokenType.RBrace,
      "[": TokenType.LBracket,
      "]": TokenType.RBracket,
      "<": TokenType.LAngle,
      ">": TokenType.RAngle,
      ",": TokenType.Comma,
      ".": TokenType.Dot,
      ":": TokenType.Colon,
      ";": TokenType.Semicolon,
      "=": TokenType.Eq,
      "+": TokenType.Plus,
      "-": TokenType.Minus,
      "*": TokenType.Star,
      "/": TokenType.Slash,
      "%": TokenType.Percent,
      "|": TokenType.Pipe,
      "^": TokenType.Caret,
      "_": TokenType.Underscore,
    };
    if (oneCharTokens[c]) {
      this.tokens.push({ type: oneCharTokens[c], value: c, line: startLine, col: startCol });
      this.advance();
      return true;
    }

    return false;
  }

  private addToken(type: TokenType, value: string): void {
    this.tokens.push({ type, value, line: this.line, col: this.col });
  }

  private advance(): void {
    this.pos++;
    this.col++;
  }

  private peek(offset: number): string {
    const idx = this.pos + offset;
    return idx < this.src.length ? this.src[idx] : "";
  }

  private isDigit(ch: string): boolean {
    return ch >= "0" && ch <= "9";
  }

  private isLowerAlpha(ch: string): boolean {
    return (ch >= "a" && ch <= "z") || ch === "_";
  }

  private isUpperAlpha(ch: string): boolean {
    return ch >= "A" && ch <= "Z";
  }

  private isAlphaNum(ch: string): boolean {
    return this.isLowerAlpha(ch) || this.isUpperAlpha(ch) || this.isDigit(ch) || ch === "_";
  }
}
