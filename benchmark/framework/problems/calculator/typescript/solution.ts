export type Op = "add" | "sub" | "mul" | "div";

export type Expr =
  | { tag: "lit"; value: number }
  | { tag: "binop"; op: Op; left: Expr; right: Expr }
  | { tag: "neg"; inner: Expr };

export function evalExpr(e: Expr): number {
  switch (e.tag) {
    case "lit":
      return e.value;
    case "neg":
      return -evalExpr(e.inner);
    case "binop": {
      const l = evalExpr(e.left);
      const r = evalExpr(e.right);
      switch (e.op) {
        case "add":
          return l + r;
        case "sub":
          return l - r;
        case "mul":
          return l * r;
        case "div":
          if (r === 0) throw new Error("division by zero");
          return l / r;
      }
    }
  }
}
