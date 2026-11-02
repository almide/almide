# ADR-0007: result.collect is removed; partition is the substance for all-errors collection

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0007-partition-replaces-collect.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0007-partition-replaces-collect.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0007-partition-replaces-collect.zh-CN.md)

> Non-normative reader copy of a pinned historical decision. The original remains authoritative; this is not automatically synchronized. Recorded measurements, comparisons and implementation claims are preserved, not independently revalidated. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0007-partition-replaces-collect.md)

- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: spellings of all-errors (all-errs) collection APIs — whether to retain `result.collect` / `result.collect_map`, and their relationship to `partition`.
- **Related**: [ADR-0004](../../../../adr/0004-error-branchability-doctrine.md) (precedent for documenting idioms and evidence gates — rejection of result.context), [ADR-0005](../../../../adr/0005-operators-desugar-to-stdlib.md) (definitional relationship between substance and sugar), [ADR-0006](../../../../adr/0006-fallibility-polymorphic-hofs.md) (the first-err side folds into `list.map(f)!`).
- **Background**: a prior-expectation conflict discovered while investigating the dismantling of try_ on 2026-08-05. A yes/no adjudication chose “C (remove) now + B (validate) behind an evidence gate.”

## Context

`result.collect` means the **exact opposite of Rust's strongest prior expectation**:

```almide
// Rust の collect(std docs 実引用済み): 最初の Err で打ち切り、E は E のまま
//   [Ok(2), Ok(4), Err("err!"), Ok(8)].collect() == Err("err!")

// almide の result.collect: 全走査・全エラー収集、E は List[E] に変わる
result.collect(rs)   // List[Result[T, E]] -> Result[List[T], List[E]]
```

`option.collect` agrees with Rust, creating an asymmetry in which only the result side violates expectations. Moreover, after ADR-0006, the canonical first-err form is `list.map(f)!`, so Almide will never need to offer first-err under the name “collect”.

The underlying operation already exists: `result.partition` (intrinsic). collect merely wraps partition plus one condition:

```almide
let (oks, errs) = result.partition(rs)
if list.is_empty(errs) then ok(oks) else err(errs)   // ← これが collect の全意味
```

Observed usage (2026-08-05): own tests, 2–3 fixtures, and docs only. Migration is inexpensive.

## Decision

<a id="d1-resultcollect--resultcollect_map-は-deprecated--削除"></a>

### D1. `result.collect` / `result.collect_map`: deprecate, then remove

After removal, E002 must include a self-repair hint directing the writer to the partition idiom:

```
error[E002]: undefined function 'result.collect'
  hint: 全エラー収集は partition で書きます:
        let (oks, errs) = result.partition(rs)
        if list.is_empty(errs) then ok(oks) else err(errs)
```

<a id="d2-partition-が唯一の実体2-行イディオムを-cheatsheet-に正準形として明記"></a>

### D2. `partition` is the sole underlying operation; document the two-line idiom as canonical in CHEATSHEET

Treat it on the same footing as the map_err context-prefix idiom (ADR-0004 D2). The conflicting word “collect” disappears from the result module.

<a id="d3-validate--validate_map-は証拠ゲート付きで予約今は追加しない"></a>

### D3. Reserve `validate` / `validate_map` behind an evidence gate (do not add them now)

Purpose-named sugar (the Scala cats Validated / Kotlin Arrow mapOrAccumulate lineage) is the same kind of “naming an idiom” as the rejected result.context (ADR-0004 Alternatives 7), and follows the same rule: document the idiom now; reconsider when evidence of actual harm appears (Falsifier 1). Reversing the condition (inverting the if branches of `is_empty`) is confirmed not to be caught by the type system — a possible future source of evidence.

<a id="d42026-08-07-追記-option-側は現状維持を正式決定"></a>

### D4 (added 2026-08-07): formally retain the option side as it is

The reason for removing `result.collect` was a meaning **opposite to Rust's strongest prior expectation**, not the word “collect” itself. Measurements show the option side does not violate that expectation, so the same rationale cannot remove it. Formally decide the following and close #1135 cluster 4:

- **Keep `option.collect` / `option.collect_map`**. `[some(1), none, some(3)]` stops at first-none and returns `none`, matching Rust's `Option` collect. There is no reason to remove a spelling whose name matches its meaning merely for symmetry (D1's rationale is “the name lies,” not “the name is the same”).
- **Keep E of `option.to_result` fixed to String**. Do not add generic `ok_or` (E-polymorphic): this consistently applies ADR-0002 D2 (E fixed to String) and ADR-0004 (custom E is written explicitly as `Result[T, MyError]`). Where custom E is needed, write `match o { some(v) => ok(v), none => err(MyErr(..)) }`.
- Consequently, the first-err/all-errs naming convention is resolved by **not creating a convention**: each function has an honest name for its own meaning. On the `result` side, all-errs belongs to `partition` (the underlying operation), and first-err to fallibility-polymorphic `list.map(xs, (x) => f(x)!)!` (ADR-0006).

Falsifier: reconsider if Dojo MSR measures actual examples misreading option.collect's first-none stopping as all-none collection, or examples where explicit match could not work around the absence of generic `ok_or`.

## Rationale

- **A name opposite to the prior expectation charges a tax every time**: although the type difference (List[E] versus E) lets check catch misuse, repeatedly correcting a reflexive Rust reading has a permanent cost.
- **Consistent spelling minimization**: with an underlying operation (partition) already available, sugar (collect) whose definitional relationship is undocumented is also a cleanup candidate under ADR-0005.
- **Consistency with the context precedent**: validate and context share the temptation to give an idiom a purpose name. Removing one and adding the other on the same day would require a distinction that could only be arbitrary, so both get the same evidence gate.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Rename to `collect_all`**: the root of the conflict, “collect,” remains; misreading risk is only halved. **Rejected**.
2. **Rename to `validate` immediately**: cannot explain how it differs from the rejected result.context. **Rejected** (reserved behind D3's evidence gate).
3. **Keep the name and add a documentation note**: writing “different from Rust” does not cure the reflex. **Rejected**.

## Consequences

- The result module loses two surface functions; all-errs converges on partition alone.
- Migration targets: spec tests, wasm_cross fixtures (reassign contracts in the same PR), monkey scenarios and docs. Also remove intrinsic `almide_rt_result_collect_map`.
- Leave the option side unchanged (`option.collect` matches Rust priors without a conflict; however, after ADR-0006 lands, check its relationship to the polymorphic `list.map(f)?`… form in Phase 2 design).

## Falsifier

1. **If validation tasks become frequent in dojo and measured mistakes occur in the partition idiom (such as an inverted condition)**, D3's gate opens: create an ADR adding `validate` / `validate_map` as named functions.
2. **If relevant dojo tasks show a significant MSR decline after migration to partition compared with the old collect**, restore a single function returning Result directly (choose its name together with D3).

## References

- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html) (an actual first-err quotation was obtained).
- Scala cats — [Validated](https://typelevel.org/cats/datatypes/validated.html), Kotlin Arrow — [mapOrAccumulate](https://arrow-kt.io/learn/typed-errors/working-with-typed-errors/) (D3's lineage).
- Internal: ADR-0004 (context precedent) / 0005 / 0006, observed usage above, partition intrinsic in `stdlib/result.almd`.
