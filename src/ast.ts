// AST node types for the LLM-native language

// ---- Type Expressions ----

export type TypeExpr =
  | { kind: "simple"; name: string }
  | { kind: "generic"; name: string; args: TypeExpr[] }
  | { kind: "record"; fields: FieldType[] }
  | { kind: "variant"; cases: VariantCase[] }
  | { kind: "fn"; params: TypeExpr[]; ret: TypeExpr }
  | { kind: "newtype"; inner: TypeExpr }

export interface FieldType {
  name: string;
  type: TypeExpr;
}

export type VariantCase =
  | { kind: "unit"; name: string }
  | { kind: "tuple"; name: string; fields: TypeExpr[] }
  | { kind: "record"; name: string; fields: FieldType[] }

// ---- Patterns ----

export type Pattern =
  | { kind: "wildcard" }
  | { kind: "ident"; name: string }
  | { kind: "literal"; value: Expr }
  | { kind: "constructor"; name: string; args: Pattern[] }
  | { kind: "record_pattern"; name: string; fields: FieldPattern[] }
  | { kind: "some"; inner: Pattern }
  | { kind: "none" }
  | { kind: "ok"; inner: Pattern }
  | { kind: "err"; inner: Pattern }

export interface FieldPattern {
  name: string;
  pattern?: Pattern; // if omitted, binds to same name
}

// ---- Expressions ----

export type Expr =
  | { kind: "int"; value: number; raw: string }
  | { kind: "float"; value: number }
  | { kind: "string"; value: string }
  | { kind: "interpolated_string"; value: string }
  | { kind: "bool"; value: boolean }
  | { kind: "ident"; name: string }
  | { kind: "type_name"; name: string }
  | { kind: "list"; elements: Expr[] }
  | { kind: "record"; fields: FieldInit[] }
  | { kind: "spread_record"; base: Expr; fields: FieldInit[] }
  | { kind: "call"; callee: Expr; args: Expr[]; namedArgs?: FieldInit[] }
  | { kind: "member"; object: Expr; field: string }
  | { kind: "pipe"; left: Expr; right: Expr }
  | { kind: "if"; cond: Expr; then: Expr; else_: Expr }
  | { kind: "match"; subject: Expr; arms: MatchArm[] }
  | { kind: "block"; stmts: Stmt[]; expr?: Expr }
  | { kind: "do_block"; stmts: Stmt[]; expr?: Expr }
  | { kind: "lambda"; params: LambdaParam[]; body: Expr }
  | { kind: "hole" }
  | { kind: "todo"; message: string }
  | { kind: "try"; expr: Expr }
  | { kind: "await"; expr: Expr }
  | { kind: "binary"; op: string; left: Expr; right: Expr }
  | { kind: "unary"; op: string; operand: Expr }
  | { kind: "paren"; expr: Expr }
  | { kind: "placeholder" }
  | { kind: "unit" }
  | { kind: "none" }
  | { kind: "some"; expr: Expr }
  | { kind: "ok"; expr: Expr }
  | { kind: "err"; expr: Expr }

export interface FieldInit {
  name: string;
  value: Expr;
}

export interface MatchArm {
  pattern: Pattern;
  guard?: Expr;
  body: Expr;
}

export interface LambdaParam {
  name: string;
  type?: TypeExpr;
}

// ---- Statements ----

export type Stmt =
  | { kind: "let"; name: string; type?: TypeExpr; value: Expr }
  | { kind: "let_destructure"; fields: string[]; value: Expr }
  | { kind: "var"; name: string; type?: TypeExpr; value: Expr }
  | { kind: "assign"; name: string; value: Expr }
  | { kind: "guard"; cond: Expr; else_: Expr }
  | { kind: "expr"; expr: Expr }

// ---- Declarations ----

export interface Param {
  name: string;
  type: TypeExpr;
}

export interface GenericParam {
  name: string;
  bounds?: string[];
}

export type Decl =
  | { kind: "module"; path: string[] }
  | { kind: "import"; path: string[]; names?: string[] }
  | { kind: "type"; name: string; generics?: GenericParam[]; type: TypeExpr; deriving?: string[] }
  | { kind: "fn"; name: string; async?: boolean; effect?: boolean; generics?: GenericParam[]; params: Param[]; returnType: TypeExpr; body: Expr }
  | { kind: "trait"; name: string; generics?: GenericParam[]; methods: TraitMethod[] }
  | { kind: "impl"; trait_: string; generics?: GenericParam[]; for_: string; methods: Decl[] }
  | { kind: "strict"; mode: string }
  | { kind: "test"; name: string; body: Expr }

export interface TraitMethod {
  name: string;
  async?: boolean;
  effect?: boolean;
  generics?: GenericParam[];
  params: Param[];
  returnType: TypeExpr;
}

// ---- Program ----

export interface Program {
  module?: Decl;
  imports: Decl[];
  decls: Decl[];
}
