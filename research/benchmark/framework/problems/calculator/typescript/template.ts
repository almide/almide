export type Op = "add" | "sub" | "mul" | "div";

export type Expr =
  | { tag: "lit"; value: number }
  | { tag: "binop"; op: Op; left: Expr; right: Expr }
  | { tag: "neg"; inner: Expr };

export function evalExpr(e: Expr): number {
  // TODO: implement
  // Throw new Error("division by zero") for division by zero
  throw new Error("not implemented");
}
