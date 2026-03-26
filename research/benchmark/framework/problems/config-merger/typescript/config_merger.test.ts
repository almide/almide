import { assertEquals, assertThrows } from "https://deno.land/std/assert/mod.ts";
import {
  parseConfig,
  mergeConfigs,
  serializeConfig,
  lookup,
  filterByPrefix,
} from "./solution.ts";

Deno.test("parse basic config", () => {
  assertEquals(parseConfig("host=localhost\nport=8080"), [
    ["host", "localhost"],
    ["port", "8080"],
  ]);
});

Deno.test("parse with comments and blanks", () => {
  const content = "# this is a comment\n\nhost=localhost\n# another comment\nport=8080";
  assertEquals(parseConfig(content), [["host", "localhost"], ["port", "8080"]]);
});

Deno.test("parse empty string", () => {
  assertEquals(parseConfig(""), []);
});

Deno.test("parse only comments", () => {
  assertEquals(parseConfig("# comment\n# another"), []);
});

Deno.test("parse missing equals", () => {
  assertThrows(
    () => parseConfig("host=ok\nbadline"),
    Error,
    "line 2: missing '='",
  );
});

Deno.test("parse empty key", () => {
  assertThrows(() => parseConfig("=value"), Error, "line 1: empty key");
});

Deno.test("parse duplicate key", () => {
  assertThrows(
    () => parseConfig("host=a\nhost=b"),
    Error,
    "line 2: duplicate key: host",
  );
});

Deno.test("parse value with equals", () => {
  assertEquals(parseConfig("formula=a=b+c"), [["formula", "a=b+c"]]);
});

Deno.test("merge no overlap", () => {
  assertEquals(mergeConfigs([["host", "localhost"]], [["port", "8080"]]), [
    ["host", "localhost"],
    ["port", "8080"],
  ]);
});

Deno.test("merge with override", () => {
  assertEquals(
    mergeConfigs(
      [["host", "localhost"], ["port", "3000"]],
      [["port", "8080"]],
    ),
    [["host", "localhost"], ["port", "8080"]],
  );
});

Deno.test("merge empty base", () => {
  assertEquals(mergeConfigs([], [["port", "8080"]]), [["port", "8080"]]);
});

Deno.test("merge empty overlay", () => {
  assertEquals(mergeConfigs([["host", "localhost"]], []), [
    ["host", "localhost"],
  ]);
});

Deno.test("serialize config", () => {
  assertEquals(
    serializeConfig([["host", "localhost"], ["port", "8080"]]),
    "host=localhost\nport=8080",
  );
});

Deno.test("serialize empty", () => {
  assertEquals(serializeConfig([]), "");
});

Deno.test("lookup found", () => {
  assertEquals(
    lookup([["host", "localhost"], ["port", "8080"]], "port"),
    "8080",
  );
});

Deno.test("lookup not found", () => {
  assertEquals(lookup([["host", "localhost"]], "port"), undefined);
});

Deno.test("filter by prefix", () => {
  assertEquals(
    filterByPrefix(
      [["db_host", "localhost"], ["db_port", "5432"], ["app_port", "8080"]],
      "db_",
    ),
    [["db_host", "localhost"], ["db_port", "5432"]],
  );
});

Deno.test("filter by prefix no match", () => {
  assertEquals(filterByPrefix([["app_port", "8080"]], "db_"), []);
});
