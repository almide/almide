import { assertEquals } from "https://deno.land/std/assert/mod.ts";
import { isPangram } from "./solution.ts";

Deno.test("empty sentence", () => {
  assertEquals(isPangram(""), false);
});

Deno.test("perfect lower case", () => {
  assertEquals(isPangram("abcdefghijklmnopqrstuvwxyz"), true);
});

Deno.test("only lower case", () => {
  assertEquals(isPangram("the quick brown fox jumps over the lazy dog"), true);
});

Deno.test("missing letter x", () => {
  assertEquals(isPangram("a quick movement of the enemy will jeopardize five gunboats"), false);
});

Deno.test("missing letter h", () => {
  assertEquals(isPangram("five boxing wizards jump quickly at my request"), false);
});

Deno.test("with underscores", () => {
  assertEquals(isPangram("the_quick_brown_fox_jumps_over_the_lazy_dog"), true);
});

Deno.test("with numbers", () => {
  assertEquals(isPangram("the 1 quick brown fox jumps over the 2 lazy dogs"), true);
});

Deno.test("missing letters replaced by numbers", () => {
  assertEquals(isPangram("7h3 qu1ck brown fox jumps ov3r 7h3 lazy dog"), false);
});

Deno.test("mixed case and punctuation", () => {
  assertEquals(isPangram('"Five quacking Zephyrs jolt my wax bed."'), true);
});

Deno.test("upper and lower case", () => {
  assertEquals(isPangram("the quick brown fox jumps over with lazy FX"), false);
});
