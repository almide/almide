<a id="als--コレクションcollections"></a>

# ALS — Collections

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](collections.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](collections.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](collections.zh-CN.md)

> Authoritative source: [docs/specs/als/collections.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot, not a replacement for the original ALS or checker/ledger/generator input; it is not automatically synchronized. Requirements, historical status, evidence and safety claims keep the source scope. No independent execution, measurement, certification or proof was performed for this edition. The source-language edition is a reader copy; other languages are translations.

> Source caveat: ALS-C5 gives the general value-semantics rule for function arguments without discussing `mut` parameters here. It is preserved, not extended to deny the separate explicit write-back contract. Constant-time claims keep the scope of ALS-T31; this edition adds no length-independence or whole-program timing guarantee.


> Last updated: 2026-10-27

Observable requirements for collection types (List / Map / Set / tuples / records). References follow the same method as the beginning of [strings.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md): the contract ledger cites them through its `spec` key.

<a id="als-c1-map-の順序規範"></a>

## ALS-C1 Map ordering requirements

Map is an **ordered dictionary preserving insertion order**. `keys`, `values`, `entries`, and iteration always follow insertion order. Reassigning an existing key does not change its position. Reinserting after deletion appends at the end. An implementation dependent on hash order or sorted order is nonconforming.
Contracts: C-013.

<a id="als-c2-set-の順序規範"></a>

## ALS-C2 Set ordering requirements

Set preserves insertion order; iteration and `to_list` deterministically return insertion order. Duplicate insertion does not change the order.
Contracts: C-014.

<a id="als-c3-構造的等価"></a>

## ALS-C3 Structural equality

`==`/`!=` are **structural deep equality**: lists, tuples, records, Map, Set, Option/Result, and nested compound values are compared recursively element by element. Reference (pointer) comparison is nonconforming. Float components use numeric comparison, not IEEE-754 bit comparison (NaN != NaN).
`==` for `Secret[String]` and `Secret[Bytes]` compares their contents in constant time (ALS-T31). `==` for other `Secret[A]` types, or for variants, Map, or Set containing Secret, is rejected at check time with E103 (`spec/wasm_cross/secret_eq_and_decode.almd`).
Contracts: C-015, C-426.

<a id="als-c4-範囲外アクセスの縮退規則"></a>

## ALS-C4 Out-of-range fallback rules

Out-of-range reads **fall back safely**: `list.get` returns none, `slice` clamps the range, and `take`/`drop` fall back to empty/the whole collection. Negative values are **read as 0** (ALS-T16's negative-count rule, almide/almide#4147): negative start and negative end in `slice` are both 0; `take(xs, 負)` is empty and `drop(xs, 負)` is the whole collection (負 denotes a negative value).
`list_slice_oob.almd` and `negative_count_rule.almd` pin this behavior. The previous rule, “negative start → empty, negative end → len, `take(xs, 負)` returns the whole collection,” came from reinterpretation via `as usize` and was withdrawn because `?limit=-1` removed the count limit.
A reversed range is empty. None of these may access outside the heap or trap. (ALS-T6 separately specifies the out-of-range abort of indexing **syntax** `xs[i]`.)
Contracts: C-034.

<a id="als-c5-値意味論copy-on-write"></a>

## ALS-C5 Value semantics (copy-on-write)

After `var b = a`, mutation of `b` is **not observable** through `a` (value semantics). An implementation may share storage through copy-on-write or similar techniques, but observable changes through an alias are nonconforming. The same applies to function arguments: a callee's mutation is not visible to the caller.
Contracts: C-033, C-150.

<a id="als-c6-型変換コンビネータ"></a>

## ALS-C6 Type-changing combinators

`map.map` and `set.map` **can change** the element type (`Map[K,A] → Map[K,B]`, `Set[A] → Set[B]`). The resulting collection follows the equality and ordering requirements for the transformed type (ALS-C1/C2/C3).
Contracts: C-039.

<a id="als-c7-レコード変種パターンマッチ"></a>

## ALS-C7 Records, variants, and pattern matching

The evaluation results of constructing records (named / anonymous) and variants (ADTs), reading fields, and pattern matching (literal / ctor / tuple / guard / nested) are observationally equivalent across targets. Matching executes the first matching arm, in top-to-bottom order.
Contracts: C-036.

<a id="als-c8-幅指定整数フィールド"></a>

## ALS-C8 Sized-integer fields

Literals placed in sized-integer fields (Int8/16/32, etc.) are **narrowed to the declared width** (C-038 promises representation narrowing for in-range literals). Literals outside the declared range are rejected with **E024**, not wrapped (ALS-M14's range discipline).
Reading returns the value at its declared width (`Int8` remains `Int8`), with no implicit numeric widening; explicitly convert with `int.from(x)` where `Int` is required (measured on 0.59.1).
Contracts: C-038.

<a id="als-c9-順序系コンビネータの全域性"></a>

## ALS-C9 Totality of ordering combinators

`list.min/max/sort/sort_by/unique_by` are total in an **element-type-directed** manner: Int/String use a total order; **Float uses IEEE-754 totalOrder** (NaN, regardless of sign or origin, is the single +qNaN 0x7FF8000000000000, the maximum after +inf; -0 < +0). They complete deterministically even with NaN present (panic or undefined order is nonconforming). `sort_by` with String keys is total with stable ordering even when elements are heap values (the result co-owns each element).
Contracts: C-053, C-055, C-276.

<a id="als-c10-空コレクションの型要件静的規範"></a>

## ALS-C10 Type requirements for empty collections (static requirement)

An empty collection whose element type cannot be inferred (such as insufficient type information for a fold over bare `[]`) is a **compile error**, not arbitrary runtime behavior. A fold's initial value is unified with the **accumulator type**, independently of the element type: `list.fold([1,2,3], "", …)` conforms and returns "123". Empty collections require an element-type annotation.
Contracts: C-052, C-058.
