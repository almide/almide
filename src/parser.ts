import { Token, TokenType } from "./lexer.ts";
import type {
  Program, Decl, Stmt, Expr, TypeExpr, Pattern, MatchArm,
  Param, GenericParam, LambdaParam, FieldInit, FieldType,
  VariantCase, TraitMethod, FieldPattern,
} from "./ast.ts";

export class ParseError extends Error {
  constructor(message: string, public token: Token) {
    super(`${message} at line ${token.line}:${token.col} (got ${token.type} '${token.value}')`);
  }
}

export class Parser {
  private tokens: Token[];
  private pos: number = 0;

  constructor(tokens: Token[]) {
    // Filter out consecutive newlines, keep only meaningful ones
    this.tokens = tokens;
  }

  parse(): Program {
    const program: Program = { imports: [], decls: [] };

    this.skipNewlines();

    // Module declaration (optional)
    if (this.check(TokenType.Module)) {
      program.module = this.parseModuleDecl();
      this.skipNewlines();
    }

    // Import declarations
    while (this.check(TokenType.Import)) {
      program.imports.push(this.parseImportDecl());
      this.skipNewlines();
    }

    // Top-level declarations
    while (!this.check(TokenType.EOF)) {
      this.skipNewlines();
      if (this.check(TokenType.EOF)) break;
      program.decls.push(this.parseTopDecl());
      this.skipNewlines();
    }

    return program;
  }

  // ---- Module & Import ----

  private parseModuleDecl(): Decl {
    this.expect(TokenType.Module);
    const path = this.parseModulePath();
    return { kind: "module", path };
  }

  private parseImportDecl(): Decl {
    this.expect(TokenType.Import);
    const path = this.parseModulePath();

    // Check for selective import: import foo.{Bar, Baz}
    if (this.check(TokenType.Dot) && this.peekAt(1)?.type === TokenType.LBrace) {
      this.advance(); // skip .
      this.expect(TokenType.LBrace);
      const names: string[] = [];
      names.push(this.expectAnyName());
      while (this.check(TokenType.Comma)) {
        this.advance();
        if (this.check(TokenType.RBrace)) break;
        names.push(this.expectAnyName());
      }
      this.expect(TokenType.RBrace);
      return { kind: "import", path, names };
    }

    return { kind: "import", path };
  }

  private parseModulePath(): string[] {
    const parts: string[] = [];
    parts.push(this.expectIdent());
    while (this.check(TokenType.Dot) && this.peekAt(1)?.type === TokenType.Ident) {
      this.advance();
      parts.push(this.expectIdent());
    }
    return parts;
  }

  // ---- Top-level Declarations ----

  private parseTopDecl(): Decl {
    if (this.check(TokenType.Type)) return this.parseTypeDecl();
    if (this.check(TokenType.Trait)) return this.parseTraitDecl();
    if (this.check(TokenType.Impl)) return this.parseImplDecl();
    if (this.check(TokenType.Fn) || this.check(TokenType.Pub) || this.check(TokenType.Effect) || this.check(TokenType.Async)) return this.parseFnDecl();
    if (this.check(TokenType.Strict)) return this.parseStrictDecl();
    if (this.check(TokenType.Test)) return this.parseTestDecl();
    throw new ParseError("Expected top-level declaration", this.current());
  }

  private parseTypeDecl(): Decl {
    this.expect(TokenType.Type);
    const name = this.expectTypeName();
    const generics = this.tryParseGenericParams();
    this.expect(TokenType.Eq);
    this.skipNewlines();
    const type = this.parseTypeExpr();
    // Check for deriving clause
    this.skipNewlines();
    let deriving: string[] | undefined;
    if (this.check(TokenType.Deriving)) {
      this.advance();
      deriving = [];
      deriving.push(this.expectTypeName());
      while (this.check(TokenType.Comma)) {
        this.advance();
        deriving.push(this.expectTypeName());
      }
    }
    return { kind: "type", name, generics: generics ?? undefined, type, deriving };
  }

  private parseTraitDecl(): Decl {
    this.expect(TokenType.Trait);
    const name = this.expectTypeName();
    const generics = this.tryParseGenericParams();
    this.expect(TokenType.LBrace);
    this.skipNewlines();
    const methods: TraitMethod[] = [];
    while (!this.check(TokenType.RBrace)) {
      methods.push(this.parseTraitMethod());
      this.skipNewlines();
    }
    this.expect(TokenType.RBrace);
    return { kind: "trait", name, generics: generics ?? undefined, methods };
  }

  private parseTraitMethod(): TraitMethod {
    let async_ = false;
    if (this.check(TokenType.Async)) {
      this.advance();
      async_ = true;
    }
    let effect = false;
    if (this.check(TokenType.Effect)) {
      this.advance();
      effect = true;
    }
    this.expect(TokenType.Fn);
    const name = this.expectAnyFnName();
    const generics = this.tryParseGenericParams();
    this.expect(TokenType.LParen);
    const params = this.parseParamList();
    this.expect(TokenType.RParen);
    this.expect(TokenType.Arrow);
    const returnType = this.parseTypeExpr();
    return { name, async: async_ || undefined, effect: effect || undefined, generics: generics ?? undefined, params, returnType };
  }

  private parseImplDecl(): Decl {
    this.expect(TokenType.Impl);
    const traitName = this.expectTypeName();
    const generics = this.tryParseGenericParams();
    this.expect(TokenType.For);
    const forName = this.expectTypeName();
    // Skip generic args on for type if present
    if (this.check(TokenType.LBracket)) {
      this.parseTypeArgs(); // consume but we don't store on impl for now
    }
    this.expect(TokenType.LBrace);
    this.skipNewlines();
    const methods: Decl[] = [];
    while (!this.check(TokenType.RBrace)) {
      methods.push(this.parseFnDecl());
      this.skipNewlines();
    }
    this.expect(TokenType.RBrace);
    return { kind: "impl", trait_: traitName, generics: generics ?? undefined, for_: forName, methods };
  }

  private parseFnDecl(): Decl {
    // optional pub
    if (this.check(TokenType.Pub)) this.advance();
    // optional async
    let async_ = false;
    if (this.check(TokenType.Async)) {
      this.advance();
      async_ = true;
    }
    // optional effect
    let effect = false;
    if (this.check(TokenType.Effect)) {
      this.advance();
      effect = true;
    }
    this.expect(TokenType.Fn);
    const name = this.expectAnyFnName();
    const generics = this.tryParseGenericParams();
    this.expect(TokenType.LParen);
    const params = this.parseParamList();
    this.expect(TokenType.RParen);
    this.expect(TokenType.Arrow);
    const returnType = this.parseTypeExpr();
    this.expect(TokenType.Eq);
    this.skipNewlines();
    const body = this.parseExpr();
    return { kind: "fn", name, async: async_ || undefined, effect: effect || undefined, generics: generics ?? undefined, params, returnType, body };
  }

  private parseStrictDecl(): Decl {
    this.expect(TokenType.Strict);
    const mode = this.expectIdent();
    return { kind: "strict", mode };
  }

  private parseTestDecl(): Decl {
    this.expect(TokenType.Test);
    const name = this.current().value;
    this.expect(TokenType.String);
    const body = this.parseBraceExpr();
    return { kind: "test", name, body };
  }

  // ---- Params ----

  private parseParamList(): Param[] {
    const params: Param[] = [];
    if (this.check(TokenType.RParen)) return params;

    // Handle 'self' as first param
    if (this.checkIdent("self")) {
      params.push({ name: "self", type: { kind: "simple", name: "Self" } });
      this.advance();
      if (this.check(TokenType.Comma)) {
        this.advance();
      }
    }

    while (!this.check(TokenType.RParen)) {
      const paramName = this.expectAnyParamName();
      this.expect(TokenType.Colon);
      const paramType = this.parseTypeExpr();
      params.push({ name: paramName, type: paramType });
      if (this.check(TokenType.Comma)) {
        this.advance();
      } else {
        break;
      }
    }
    return params;
  }

  // ---- Types ----

  private parseTypeExpr(): TypeExpr {
    // Check for newtype
    if (this.check(TokenType.Newtype)) {
      this.advance();
      const inner = this.parseTypeExpr();
      return { kind: "newtype", inner };
    }

    // Check for variant type (starts with |)
    if (this.check(TokenType.Pipe)) {
      return this.parseVariantType();
    }

    // Check for record type
    if (this.check(TokenType.LBrace)) {
      return this.parseRecordType();
    }

    // Check for fn type
    if (this.check(TokenType.Fn)) {
      return this.parseFnType();
    }

    // Simple or generic type (may start an inline variant)
    const name = this.expectTypeName();
    if (this.check(TokenType.LBracket)) {
      const args = this.parseTypeArgs();
      // Check for inline variant: Name<T> | ...
      if (this.check(TokenType.Pipe)) {
        return this.tryParseInlineVariant(name, []);
      }
      return { kind: "generic", name, args };
    }
    // Check for inline variant: Name(T) | Name2(T2)
    if (this.check(TokenType.LParen)) {
      this.advance();
      const fields: TypeExpr[] = [];
      if (!this.check(TokenType.RParen)) {
        fields.push(this.parseTypeExpr());
        while (this.check(TokenType.Comma)) {
          this.advance();
          fields.push(this.parseTypeExpr());
        }
      }
      this.expect(TokenType.RParen);
      // If followed by |, it's an inline variant
      if (this.check(TokenType.Pipe)) {
        return this.tryParseInlineVariant(name, fields);
      }
      // Otherwise just a simple type with args (shouldn't happen in type position normally)
      return { kind: "simple", name };
    }
    // Check for unit inline variant: Name | Name2
    if (this.check(TokenType.Pipe)) {
      return this.tryParseInlineVariant(name, []);
    }
    return { kind: "simple", name };
  }

  private parseVariantType(): TypeExpr {
    const cases: VariantCase[] = [];
    while (this.check(TokenType.Pipe)) {
      this.advance(); // skip |
      this.skipNewlines();
      const caseName = this.expectTypeName();
      if (this.check(TokenType.LParen)) {
        this.advance();
        const fields: TypeExpr[] = [];
        if (!this.check(TokenType.RParen)) {
          fields.push(this.parseTypeExpr());
          while (this.check(TokenType.Comma)) {
            this.advance();
            fields.push(this.parseTypeExpr());
          }
        }
        this.expect(TokenType.RParen);
        cases.push({ kind: "tuple", name: caseName, fields });
      } else if (this.check(TokenType.LBrace)) {
        this.advance();
        const fields = this.parseFieldTypeList();
        this.expect(TokenType.RBrace);
        cases.push({ kind: "record", name: caseName, fields });
      } else {
        cases.push({ kind: "unit", name: caseName });
      }
      this.skipNewlines();
    }
    return { kind: "variant", cases };
  }

  // Also support inline variant: Io(IoError) | Parse(ParseError)
  private tryParseInlineVariant(firstName: string, firstArgs: TypeExpr[]): TypeExpr {
    const cases: VariantCase[] = [];
    cases.push(
      firstArgs.length > 0
        ? { kind: "tuple", name: firstName, fields: firstArgs }
        : { kind: "unit", name: firstName }
    );
    while (this.check(TokenType.Pipe)) {
      this.advance();
      this.skipNewlines();
      const caseName = this.expectTypeName();
      if (this.check(TokenType.LParen)) {
        this.advance();
        const fields: TypeExpr[] = [];
        if (!this.check(TokenType.RParen)) {
          fields.push(this.parseTypeExpr());
          while (this.check(TokenType.Comma)) {
            this.advance();
            fields.push(this.parseTypeExpr());
          }
        }
        this.expect(TokenType.RParen);
        cases.push({ kind: "tuple", name: caseName, fields });
      } else if (this.check(TokenType.LBrace)) {
        this.advance();
        const fields = this.parseFieldTypeList();
        this.expect(TokenType.RBrace);
        cases.push({ kind: "record", name: caseName, fields });
      } else {
        cases.push({ kind: "unit", name: caseName });
      }
      this.skipNewlines();
    }
    return { kind: "variant", cases };
  }

  private parseRecordType(): TypeExpr {
    this.expect(TokenType.LBrace);
    this.skipNewlines();
    const fields = this.parseFieldTypeList();
    this.skipNewlines();
    this.expect(TokenType.RBrace);
    return { kind: "record", fields };
  }

  private parseFieldTypeList(): FieldType[] {
    const fields: FieldType[] = [];
    while (!this.check(TokenType.RBrace)) {
      this.skipNewlines();
      const fieldName = this.expectIdent();
      this.expect(TokenType.Colon);
      const fieldType = this.parseTypeExpr();
      fields.push({ name: fieldName, type: fieldType });
      this.skipNewlines();
      if (this.check(TokenType.Comma)) {
        this.advance();
        this.skipNewlines();
      }
    }
    return fields;
  }

  private parseFnType(): TypeExpr {
    this.expect(TokenType.Fn);
    this.expect(TokenType.LParen);
    const params: TypeExpr[] = [];
    if (!this.check(TokenType.RParen)) {
      params.push(this.parseTypeExpr());
      while (this.check(TokenType.Comma)) {
        this.advance();
        params.push(this.parseTypeExpr());
      }
    }
    this.expect(TokenType.RParen);
    this.expect(TokenType.Arrow);
    const ret = this.parseTypeExpr();
    return { kind: "fn", params, ret };
  }

  private parseTypeArgs(): TypeExpr[] {
    this.expect(TokenType.LBracket);
    const args: TypeExpr[] = [];
    if (!this.check(TokenType.RBracket)) {
      args.push(this.parseTypeExpr());
      while (this.check(TokenType.Comma)) {
        this.advance();
        args.push(this.parseTypeExpr());
      }
    }
    this.expect(TokenType.RBracket);
    return args;
  }

  private tryParseGenericParams(): GenericParam[] | null {
    if (!this.check(TokenType.LBracket)) return null;
    this.advance();
    const params: GenericParam[] = [];
    if (!this.check(TokenType.RBracket)) {
      params.push(this.parseGenericParam());
      while (this.check(TokenType.Comma)) {
        this.advance();
        params.push(this.parseGenericParam());
      }
    }
    this.expect(TokenType.RBracket);
    return params;
  }

  private parseGenericParam(): GenericParam {
    const name = this.expectTypeName();
    const bounds: string[] = [];
    if (this.check(TokenType.Colon)) {
      this.advance();
      bounds.push(this.expectTypeName());
      while (this.check(TokenType.Plus)) {
        this.advance();
        bounds.push(this.expectTypeName());
      }
    }
    return { name, bounds: bounds.length > 0 ? bounds : undefined };
  }

  // ---- Statements ----

  private parseStmt(): Stmt {
    if (this.check(TokenType.Let)) return this.parseLetStmt();
    if (this.check(TokenType.Var)) return this.parseVarStmt();
    if (this.check(TokenType.Guard)) return this.parseGuardStmt();

    // Try assign: ident = expr
    if (this.check(TokenType.Ident) &&
        this.peekAt(1)?.type === TokenType.Eq &&
        this.peekAt(2)?.type !== TokenType.Eq) {
      return this.parseAssignStmt();
    }

    const expr = this.parseExpr();
    return { kind: "expr", expr };
  }

  private parseLetStmt(): Stmt {
    this.expect(TokenType.Let);

    // Destructuring: let { a, b } = expr
    if (this.check(TokenType.LBrace)) {
      this.advance();
      const fields: string[] = [];
      while (!this.check(TokenType.RBrace)) {
        fields.push(this.expectIdent());
        if (this.check(TokenType.Comma)) {
          this.advance();
          this.skipNewlines();
        }
      }
      this.expect(TokenType.RBrace);
      this.expect(TokenType.Eq);
      this.skipNewlines();
      const value = this.parseExpr();
      return { kind: "let_destructure", fields, value };
    }

    const name = this.expectIdent();
    let type: TypeExpr | undefined;
    if (this.check(TokenType.Colon)) {
      this.advance();
      type = this.parseTypeExpr();
    }
    this.expect(TokenType.Eq);
    this.skipNewlines();
    const value = this.parseExpr();
    return { kind: "let", name, type, value };
  }

  private parseVarStmt(): Stmt {
    this.expect(TokenType.Var);
    const name = this.expectIdent();
    let type: TypeExpr | undefined;
    if (this.check(TokenType.Colon)) {
      this.advance();
      type = this.parseTypeExpr();
    }
    this.expect(TokenType.Eq);
    this.skipNewlines();
    const value = this.parseExpr();
    return { kind: "var", name, type, value };
  }

  private parseGuardStmt(): Stmt {
    this.expect(TokenType.Guard);
    const cond = this.parseExpr();
    this.expect(TokenType.Else);
    this.skipNewlines();
    const else_ = this.parseExpr();
    return { kind: "guard", cond, else_ };
  }

  private parseAssignStmt(): Stmt {
    const name = this.current().value;
    this.advance();
    this.expect(TokenType.Eq);
    this.skipNewlines();
    const value = this.parseExpr();
    return { kind: "assign", name, value };
  }

  // ---- Expressions ----

  private parseExpr(): Expr {
    return this.parsePipe();
  }

  private parsePipe(): Expr {
    let left = this.parseOr();
    while (this.check(TokenType.PipeArrow)) {
      this.advance();
      this.skipNewlines();
      const right = this.parseOr();
      left = { kind: "pipe", left, right };
    }
    return left;
  }

  private parseOr(): Expr {
    let left = this.parseAnd();
    while (this.check(TokenType.Or)) {
      this.advance();
      this.skipNewlines();
      const right = this.parseAnd();
      left = { kind: "binary", op: "or", left, right };
    }
    return left;
  }

  private parseAnd(): Expr {
    let left = this.parseComparison();
    while (this.check(TokenType.And)) {
      this.advance();
      this.skipNewlines();
      const right = this.parseComparison();
      left = { kind: "binary", op: "and", left, right };
    }
    return left;
  }

  private parseComparison(): Expr {
    let left = this.parseAddSub();
    const ops = [TokenType.EqEq, TokenType.BangEq, TokenType.LAngle, TokenType.RAngle, TokenType.LtEq, TokenType.GtEq];
    while (ops.some(op => this.check(op))) {
      const op = this.current().value;
      this.advance();
      this.skipNewlines();
      const right = this.parseAddSub();
      left = { kind: "binary", op, left, right };
    }
    return left;
  }

  private parseAddSub(): Expr {
    let left = this.parseMulDiv();
    while (this.check(TokenType.Plus) || this.check(TokenType.Minus) || this.check(TokenType.PlusPlus)) {
      const op = this.current().value;
      this.advance();
      this.skipNewlines();
      const right = this.parseMulDiv();
      left = { kind: "binary", op, left, right };
    }
    return left;
  }

  private parseMulDiv(): Expr {
    let left = this.parseUnary();
    while (this.check(TokenType.Star) || this.check(TokenType.Slash) || this.check(TokenType.Percent) || this.check(TokenType.Caret)) {
      const op = this.current().value;
      this.advance();
      this.skipNewlines();
      const right = this.parseUnary();
      left = { kind: "binary", op, left, right };
    }
    return left;
  }

  private parseUnary(): Expr {
    if (this.check(TokenType.Minus)) {
      this.advance();
      const operand = this.parseUnary();
      return { kind: "unary", op: "-", operand };
    }
    if (this.check(TokenType.Not)) {
      this.advance();
      const operand = this.parseUnary();
      return { kind: "unary", op: "not", operand };
    }
    return this.parsePostfix();
  }

  private parsePostfix(): Expr {
    let expr = this.parsePrimary();

    while (true) {
      if (this.check(TokenType.Dot)) {
        this.advance();
        const field = this.expectAnyName();
        expr = { kind: "member", object: expr, field };
      } else if (this.check(TokenType.LParen)) {
        this.advance();
        const { args, namedArgs } = this.parseCallArgs();
        this.expect(TokenType.RParen);
        expr = { kind: "call", callee: expr, args, namedArgs };
      } else {
        break;
      }
    }

    return expr;
  }

  private parseArgList(): Expr[] {
    const args: Expr[] = [];
    if (this.check(TokenType.RParen)) return args;
    args.push(this.parseExpr());
    while (this.check(TokenType.Comma)) {
      this.advance();
      this.skipNewlines();
      if (this.check(TokenType.RParen)) break;
      args.push(this.parseExpr());
    }
    return args;
  }

  private parseCallArgs(): { args: Expr[]; namedArgs?: FieldInit[] } {
    const args: Expr[] = [];
    const namedArgs: FieldInit[] = [];
    if (this.check(TokenType.RParen)) return { args };

    this.parseOneCallArg(args, namedArgs);
    while (this.check(TokenType.Comma)) {
      this.advance();
      this.skipNewlines();
      if (this.check(TokenType.RParen)) break;
      this.parseOneCallArg(args, namedArgs);
    }
    return { args, namedArgs: namedArgs.length > 0 ? namedArgs : undefined };
  }

  private parseOneCallArg(args: Expr[], namedArgs: FieldInit[]): void {
    // Placeholder: _ in call args
    if (this.check(TokenType.Underscore)) {
      this.advance();
      args.push({ kind: "placeholder" });
      return;
    }
    // Named arg: ident ":" expr
    if (this.check(TokenType.Ident) && this.peekAt(1)?.type === TokenType.Colon) {
      const name = this.advance().value;
      this.advance(); // skip :
      this.skipNewlines();
      const value = this.parseExpr();
      namedArgs.push({ name, value });
    } else {
      args.push(this.parseExpr());
    }
  }

  private parsePrimary(): Expr {
    const tok = this.current();

    // Literals
    if (this.check(TokenType.Int)) {
      this.advance();
      return { kind: "int", value: parseInt(tok.value), raw: tok.value };
    }
    if (this.check(TokenType.Float)) {
      this.advance();
      return { kind: "float", value: parseFloat(tok.value) };
    }
    if (this.check(TokenType.String)) {
      this.advance();
      return { kind: "string", value: tok.value };
    }
    if (this.check(TokenType.InterpolatedString)) {
      this.advance();
      return { kind: "interpolated_string", value: tok.value };
    }
    if (this.check(TokenType.True)) {
      this.advance();
      return { kind: "bool", value: true };
    }
    if (this.check(TokenType.False)) {
      this.advance();
      return { kind: "bool", value: false };
    }

    // Hole
    if (this.check(TokenType.Underscore)) {
      this.advance();
      return { kind: "hole" };
    }

    // None
    if (this.check(TokenType.None)) {
      this.advance();
      return { kind: "none" };
    }

    // Some(expr)
    if (this.check(TokenType.Some)) {
      this.advance();
      this.expect(TokenType.LParen);
      const expr = this.parseExpr();
      this.expect(TokenType.RParen);
      return { kind: "some", expr };
    }

    // Ok(expr)
    if (this.check(TokenType.Ok)) {
      this.advance();
      this.expect(TokenType.LParen);
      const expr = this.parseExpr();
      this.expect(TokenType.RParen);
      return { kind: "ok", expr };
    }

    // Err(expr)
    if (this.check(TokenType.Err)) {
      this.advance();
      this.expect(TokenType.LParen);
      const expr = this.parseExpr();
      this.expect(TokenType.RParen);
      return { kind: "err", expr };
    }

    // Todo
    if (this.check(TokenType.Todo)) {
      this.advance();
      this.expect(TokenType.LParen);
      const msg = this.current().value;
      this.expect(TokenType.String);
      this.expect(TokenType.RParen);
      return { kind: "todo", message: msg };
    }

    // Try
    if (this.check(TokenType.Try)) {
      this.advance();
      const expr = this.parsePostfix();
      return { kind: "try", expr };
    }

    // Await
    if (this.check(TokenType.Await)) {
      this.advance();
      const expr = this.parsePostfix();
      return { kind: "await", expr };
    }

    // If
    if (this.check(TokenType.If)) {
      return this.parseIfExpr();
    }

    // Match
    if (this.check(TokenType.Match)) {
      return this.parseMatchExpr();
    }

    // Lambda: fn(...) => expr
    if (this.check(TokenType.Fn) && this.peekAt(1)?.type === TokenType.LParen) {
      return this.parseLambda();
    }

    // Do block
    if (this.check(TokenType.Do)) {
      this.advance();
      return this.parseDoBlock();
    }

    // Block or Record/Spread
    if (this.check(TokenType.LBrace)) {
      return this.parseBraceExpr();
    }

    // List
    if (this.check(TokenType.LBracket)) {
      return this.parseListExpr();
    }

    // Paren or Unit ()
    if (this.check(TokenType.LParen)) {
      this.advance();
      if (this.check(TokenType.RParen)) {
        this.advance();
        return { kind: "unit" };
      }
      const expr = this.parseExpr();
      this.expect(TokenType.RParen);
      return { kind: "paren", expr };
    }

    // Type constructor (call)
    if (this.check(TokenType.TypeName)) {
      const name = tok.value;
      this.advance();

      // Generic call: TypeName[...](...)
      if (this.check(TokenType.LBracket)) {
        this.parseTypeArgs(); // consume type args
        if (this.check(TokenType.LParen)) {
          this.advance();
          const { args, namedArgs } = this.parseCallArgs();
          this.expect(TokenType.RParen);
          return { kind: "call", callee: { kind: "type_name", name }, args, namedArgs };
        }
        return { kind: "type_name", name };
      }

      // Simple call: TypeName(...)
      if (this.check(TokenType.LParen)) {
        this.advance();
        const { args, namedArgs } = this.parseCallArgs();
        this.expect(TokenType.RParen);
        return { kind: "call", callee: { kind: "type_name", name }, args, namedArgs };
      }

      return { kind: "type_name", name };
    }

    // Identifier (with ? or !)
    if (this.check(TokenType.Ident) || this.check(TokenType.IdentQ)) {
      const name = tok.value;
      this.advance();
      return { kind: "ident", name };
    }

    throw new ParseError("Expected expression", tok);
  }

  private parseIfExpr(): Expr {
    this.expect(TokenType.If);
    this.skipNewlines();
    const cond = this.parseExpr();
    this.skipNewlines();
    this.expect(TokenType.Then);
    this.skipNewlines();
    const then = this.parseExpr();
    this.skipNewlines();
    this.expect(TokenType.Else);
    this.skipNewlines();
    const else_ = this.parseExpr();
    return { kind: "if", cond, then, else_ };
  }

  private parseMatchExpr(): Expr {
    this.expect(TokenType.Match);
    this.skipNewlines();
    const subject = this.parsePostfix();
    this.skipNewlines();
    this.expect(TokenType.LBrace);
    this.skipNewlines();
    const arms: MatchArm[] = [];
    while (!this.check(TokenType.RBrace)) {
      arms.push(this.parseMatchArm());
      this.skipNewlines();
      if (this.check(TokenType.Comma)) {
        this.advance();
        this.skipNewlines();
      }
    }
    this.expect(TokenType.RBrace);
    return { kind: "match", subject, arms };
  }

  private parseMatchArm(): MatchArm {
    const pattern = this.parsePattern();
    let guard: Expr | undefined;
    if (this.check(TokenType.If)) {
      this.advance();
      guard = this.parseExpr();
    }
    this.expect(TokenType.FatArrow);
    this.skipNewlines();
    const body = this.parseExpr();
    return { pattern, guard, body };
  }

  private parsePattern(): Pattern {
    // Wildcard
    if (this.check(TokenType.Underscore)) {
      this.advance();
      return { kind: "wildcard" };
    }

    // none
    if (this.check(TokenType.None)) {
      this.advance();
      return { kind: "none" };
    }

    // some(pattern)
    if (this.check(TokenType.Some)) {
      this.advance();
      this.expect(TokenType.LParen);
      const inner = this.parsePattern();
      this.expect(TokenType.RParen);
      return { kind: "some", inner };
    }

    // ok(pattern)
    if (this.check(TokenType.Ok)) {
      this.advance();
      this.expect(TokenType.LParen);
      const inner = this.parsePattern();
      this.expect(TokenType.RParen);
      return { kind: "ok", inner };
    }

    // err(pattern)
    if (this.check(TokenType.Err)) {
      this.advance();
      this.expect(TokenType.LParen);
      const inner = this.parsePattern();
      this.expect(TokenType.RParen);
      return { kind: "err", inner };
    }

    // Literal patterns
    if (this.check(TokenType.Int) || this.check(TokenType.Float) || this.check(TokenType.String)) {
      const expr = this.parsePrimary();
      return { kind: "literal", value: expr };
    }
    if (this.check(TokenType.True)) {
      this.advance();
      return { kind: "literal", value: { kind: "bool", value: true } };
    }
    if (this.check(TokenType.False)) {
      this.advance();
      return { kind: "literal", value: { kind: "bool", value: false } };
    }

    // Type constructor pattern
    if (this.check(TokenType.TypeName)) {
      const name = this.current().value;
      this.advance();
      if (this.check(TokenType.LParen)) {
        this.advance();
        const args: Pattern[] = [];
        if (!this.check(TokenType.RParen)) {
          args.push(this.parsePattern());
          while (this.check(TokenType.Comma)) {
            this.advance();
            args.push(this.parsePattern());
          }
        }
        this.expect(TokenType.RParen);
        return { kind: "constructor", name, args };
      }
      if (this.check(TokenType.LBrace)) {
        this.advance();
        this.skipNewlines();
        const fields: FieldPattern[] = [];
        while (!this.check(TokenType.RBrace)) {
          const fieldName = this.expectIdent();
          if (this.check(TokenType.Colon)) {
            this.advance();
            const pattern = this.parsePattern();
            fields.push({ name: fieldName, pattern });
          } else {
            fields.push({ name: fieldName });
          }
          if (this.check(TokenType.Comma)) {
            this.advance();
            this.skipNewlines();
          }
        }
        this.expect(TokenType.RBrace);
        return { kind: "record_pattern", name, fields };
      }
      return { kind: "constructor", name, args: [] };
    }

    // Identifier pattern
    if (this.check(TokenType.Ident)) {
      const name = this.current().value;
      this.advance();
      return { kind: "ident", name };
    }

    throw new ParseError("Expected pattern", this.current());
  }

  private parseLambda(): Expr {
    this.expect(TokenType.Fn);
    this.expect(TokenType.LParen);
    const params: LambdaParam[] = [];
    if (!this.check(TokenType.RParen)) {
      params.push(this.parseLambdaParam());
      while (this.check(TokenType.Comma)) {
        this.advance();
        params.push(this.parseLambdaParam());
      }
    }
    this.expect(TokenType.RParen);
    this.expect(TokenType.FatArrow);
    this.skipNewlines();
    const body = this.parseExpr();
    return { kind: "lambda", params, body };
  }

  private parseLambdaParam(): LambdaParam {
    const name = this.expectIdent();
    let type: TypeExpr | undefined;
    if (this.check(TokenType.Colon)) {
      this.advance();
      type = this.parseTypeExpr();
    }
    return { name, type };
  }

  private parseDoBlock(): Expr {
    this.expect(TokenType.LBrace);
    this.skipNewlines();
    const stmts: Stmt[] = [];
    let finalExpr: Expr | undefined;

    while (!this.check(TokenType.RBrace)) {
      const stmt = this.parseStmt();
      this.skipNewlines();
      if (this.check(TokenType.Semicolon)) {
        this.advance();
        this.skipNewlines();
      }

      if (this.check(TokenType.RBrace) && stmt.kind === "expr") {
        finalExpr = stmt.expr;
      } else {
        stmts.push(stmt);
      }
    }
    this.expect(TokenType.RBrace);
    return { kind: "do_block", stmts, expr: finalExpr };
  }

  private parseBraceExpr(): Expr {
    const startPos = this.pos;

    // Try to detect if this is a record/spread vs a block
    // Record: { name: expr, ... } or { ...expr, name: expr }
    // Block: { stmt; stmt; expr }

    this.expect(TokenType.LBrace);
    this.skipNewlines();

    // Empty braces -> empty record
    if (this.check(TokenType.RBrace)) {
      this.advance();
      return { kind: "record", fields: [] };
    }

    // Spread: { ...expr, ... }
    if (this.check(TokenType.DotDotDot)) {
      this.advance();
      const base = this.parseExpr();
      const fields: FieldInit[] = [];
      while (this.check(TokenType.Comma)) {
        this.advance();
        this.skipNewlines();
        if (this.check(TokenType.RBrace)) break;
        const fieldName = this.expectIdent();
        this.expect(TokenType.Colon);
        this.skipNewlines();
        const fieldValue = this.parseExpr();
        fields.push({ name: fieldName, value: fieldValue });
      }
      this.skipNewlines();
      this.expect(TokenType.RBrace);
      return { kind: "spread_record", base, fields };
    }

    // Try to detect record: ident ":" expr
    if ((this.check(TokenType.Ident) || this.check(TokenType.IdentQ)) &&
        this.peekAt(1)?.type === TokenType.Colon) {
      // Record literal
      const fields: FieldInit[] = [];
      while (!this.check(TokenType.RBrace)) {
        this.skipNewlines();
        const fieldName = this.expectAnyName();
        if (this.check(TokenType.Colon)) {
          this.advance();
          this.skipNewlines();
          const fieldValue = this.parseExpr();
          fields.push({ name: fieldName, value: fieldValue });
        } else {
          // Shorthand: { name } == { name: name }
          fields.push({ name: fieldName, value: { kind: "ident", name: fieldName } });
        }
        this.skipNewlines();
        if (this.check(TokenType.Comma)) {
          this.advance();
          this.skipNewlines();
        }
      }
      this.expect(TokenType.RBrace);
      return { kind: "record", fields };
    }

    // Block
    const stmts: Stmt[] = [];
    let finalExpr: Expr | undefined;

    while (!this.check(TokenType.RBrace)) {
      const stmt = this.parseStmt();
      this.skipNewlines();
      if (this.check(TokenType.Semicolon)) {
        this.advance();
        this.skipNewlines();
      }

      if (this.check(TokenType.RBrace) && stmt.kind === "expr") {
        finalExpr = stmt.expr;
      } else {
        stmts.push(stmt);
      }
    }
    this.expect(TokenType.RBrace);
    return { kind: "block", stmts, expr: finalExpr };
  }

  private parseListExpr(): Expr {
    this.expect(TokenType.LBracket);
    this.skipNewlines();
    const elements: Expr[] = [];
    while (!this.check(TokenType.RBracket)) {
      elements.push(this.parseExpr());
      this.skipNewlines();
      if (this.check(TokenType.Comma)) {
        this.advance();
        this.skipNewlines();
      }
    }
    this.expect(TokenType.RBracket);
    return { kind: "list", elements };
  }

  // ---- Helpers ----

  private current(): Token {
    return this.tokens[this.pos] ?? { type: TokenType.EOF, value: "", line: 0, col: 0 };
  }

  private peekAt(offset: number): Token | undefined {
    return this.tokens[this.pos + offset];
  }

  private check(type: TokenType): boolean {
    return this.current().type === type;
  }

  private checkIdent(name: string): boolean {
    return this.current().type === TokenType.Ident && this.current().value === name;
  }

  private advance(): Token {
    const tok = this.current();
    this.pos++;
    return tok;
  }

  private expect(type: TokenType): Token {
    if (!this.check(type)) {
      throw new ParseError(`Expected ${type}`, this.current());
    }
    return this.advance();
  }

  private expectIdent(): string {
    if (this.check(TokenType.Ident)) return this.advance().value;
    throw new ParseError("Expected identifier", this.current());
  }

  private expectTypeName(): string {
    if (this.check(TokenType.TypeName)) return this.advance().value;
    throw new ParseError("Expected type name", this.current());
  }

  private expectAnyName(): string {
    if (this.check(TokenType.Ident)) return this.advance().value;
    if (this.check(TokenType.IdentQ)) return this.advance().value;
    if (this.check(TokenType.TypeName)) return this.advance().value;
    throw new ParseError("Expected name", this.current());
  }

  private expectAnyFnName(): string {
    if (this.check(TokenType.Ident)) return this.advance().value;
    if (this.check(TokenType.IdentQ)) return this.advance().value;
    throw new ParseError("Expected function name", this.current());
  }

  private expectAnyParamName(): string {
    if (this.check(TokenType.Ident)) return this.advance().value;
    // Allow var keyword as param name for 'var List<T>' style
    if (this.check(TokenType.Var)) return this.advance().value;
    throw new ParseError("Expected parameter name", this.current());
  }

  private skipNewlines(): void {
    while (this.check(TokenType.Newline)) {
      this.advance();
    }
  }
}
