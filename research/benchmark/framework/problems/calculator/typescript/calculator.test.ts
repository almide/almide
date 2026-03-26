import { assertEquals, assertThrows } from "https://deno.land/std/assert/mod.ts";
import { evalExpr, type Expr } from "./solution.ts";

const lit = (n: number): Expr => ({ tag: "lit", value: n });
const binop = (op: "add" | "sub" | "mul" | "div", left: Expr, right: Expr): Expr => ({
  tag: "binop", op, left, right,
});
const neg = (inner: Expr): Expr => ({ tag: "neg", inner });

Deno.test("literal", () => {
  assertEquals(evalExpr(lit(42.0)), 42.0);
});

Deno.test("addition", () => {
  assertEquals(evalExpr(binop("add", lit(2.0), lit(3.0))), 5.0);
});

Deno.test("subtraction", () => {
  assertEquals(evalExpr(binop("sub", lit(10.0), lit(4.0))), 6.0);
});

Deno.test("multiplication", () => {
  assertEquals(evalExpr(binop("mul", lit(3.0), lit(7.0))), 21.0);
});

Deno.test("division", () => {
  assertEquals(evalExpr(binop("div", lit(10.0), lit(4.0))), 2.5);
});

Deno.test("division by zero", () => {
  assertThrows(
    () => evalExpr(binop("div", lit(1.0), lit(0.0))),
    Error,
    "division by zero",
  );
});

Deno.test("negation", () => {
  assertEquals(evalExpr(neg(lit(5.0))), -5.0);
});

Deno.test("nested", () => {
  assertEquals(evalExpr(binop("add", lit(1.0), binop("mul", lit(2.0), lit(3.0)))), 7.0);
});

Deno.test("complex", () => {
  assertEquals(
    evalExpr(binop("sub", binop("add", lit(10.0), lit(5.0)), neg(lit(3.0)))),
    18.0,
  );
});

Deno.test("deeply nested", () => {
  assertEquals(
    evalExpr(neg(binop("mul", neg(lit(2.0)), binop("add", lit(3.0), lit(4.0))))),
    14.0,
  );
});
