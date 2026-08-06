# ADR-0007: result.collect is removed; partition is the substance for all-errors collection

- **Status**: Accepted
- **Date**: 2026-08-05
- **決定範囲**: 全エラー収集(all-errs)系 API の綴り — `result.collect` /
  `result.collect_map` の存廃と、`partition` との関係
- **関連**: [ADR-0004](./0004-error-branchability-doctrine.md)(イディオム文書化 +
  証拠ゲートの判例 — result.context 却下)、[ADR-0005](./0005-operators-desugar-to-stdlib.md)
  (実体と糖衣の定義関係)、[ADR-0006](./0006-fallibility-polymorphic-hofs.md)
  (first-err 側は `list.map(f)!` に溶ける)
- **経緯**: 2026-08-05、try_ 解体の調査中に発見した事前分布衝突から。○×裁定で
  「C(削除)今 + B(validate)は証拠ゲート」に決定。

## Context

`result.collect` は **Rust の最強事前分布と真逆**の意味を持つ:

```almide
// Rust の collect(std docs 実引用済み): 最初の Err で打ち切り、E は E のまま
//   [Ok(2), Ok(4), Err("err!"), Ok(8)].collect() == Err("err!")

// almide の result.collect: 全走査・全エラー収集、E は List[E] に変わる
result.collect(rs)   // List[Result[T, E]] -> Result[List[T], List[E]]
```

`option.collect` は Rust の挙動と一致しており、result 側だけが裏切る非対称。
さらに ADR-0006 後は first-err 側の正準形が `list.map(f)!` になるため、
almide が「collect」という名で first-err を提供する必要は恒久にない。

実体は既に存在する: `result.partition`(intrinsic)。collect が包んでいるのは
partition + 判定 1 つだけ:

```almide
let (oks, errs) = result.partition(rs)
if list.is_empty(errs) then ok(oks) else err(errs)   // ← これが collect の全意味
```

使用量実測(2026-08-05): 自前テスト・fixture 2〜3 件・docs のみ。移行は安い。

## Decision

### D1. `result.collect` / `result.collect_map` は deprecated → 削除

削除後の E002 には partition イディオムへの自己修復 hint を必須で付ける:

```
error[E002]: undefined function 'result.collect'
  hint: 全エラー収集は partition で書きます:
        let (oks, errs) = result.partition(rs)
        if list.is_empty(errs) then ok(oks) else err(errs)
```

### D2. `partition` が唯一の実体、2 行イディオムを CHEATSHEET に正準形として明記

map_err の文脈前置イディオム(ADR-0004 D2)と同格の扱い。
"collect" という衝突語は result モジュールから消滅する。

### D3. `validate` / `validate_map` は証拠ゲート付きで予約(今は追加しない)

用途名の糖衣(Scala cats Validated / Kotlin Arrow mapOrAccumulate 系譜)は、
result.context 却下(ADR-0004 Alternatives 7)と同型の「イディオムの命名」であり、
同じルールに従う: 今はイディオム文書化のみ、実害の証拠が出たら再考(Falsifier 1)。
判定条件の反転(`is_empty` の if 分岐逆転)は型で捕まらないことを確認済み —
これが将来の証拠候補。

## Rationale

- **事前分布と真逆の名前は毎回税金を取る**: 型差(List[E] vs E)で誤用は check に
  捕まるとはいえ、Rust 反射の誤読を毎回訂正させるコストは恒久
- **綴り最小化の一貫適用**: 実体(partition)がありながら定義関係の文書化されて
  いない糖衣(collect)は、ADR-0005 の原理でも整理対象
- **context 判例との一貫性**: 「イディオムに用途名を付けたい」誘惑は validate も
  context も同じ。同日に片方を殺し片方を足すなら原則が要るが、その原則は
  恣意的にしか引けない — よって同じ証拠ゲートに載せる

## Alternatives — 検討して却下した案

1. **`collect_all` へ改名**: 衝突の根 "collect" が残り、誤読リスクが半減止まり。**却下**。
2. **`validate` へ改名(即時)**: result.context 却下との区別が言えない。**却下**
   (D3 で証拠ゲート付き予約に)。
3. **名前維持 + docs で注記**: 「Rust とは違う」と書いても反射は直らない。**却下**。

## Consequences

- result モジュールの表面が 2 関数減り、all-errs は partition 1 本に収束
- 移行対象: spec テスト・wasm_cross fixture(契約の付け替えを同 PR で)・
  monkey シナリオ・docs。intrinsic `almide_rt_result_collect_map` も削除
- option 側は現状維持(`option.collect` は Rust prior と一致しており衝突がない。
  ただし ADR-0006 着地後は `list.map(f)?`… の多相形との関係を Phase 2 設計で確認)

## Falsifier

1. **dojo でバリデーション用途が頻出し、かつ partition イディオムの書き損じ
   (判定反転等)が実測された場合** — D3 のゲートが開き、`validate` /
   `validate_map` を名前付き関数として追加する ADR を起こす。
2. **partition イディオム移行後、dojo の該当タスクで MSR が旧 collect 比で有意に
   劣化した場合** — Result 型で直接受ける一関数を復活させる(名前は D3 と同時決定)。

## References

- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html)
  (first-err の実引用取得済み)
- Scala cats — [Validated](https://typelevel.org/cats/datatypes/validated.html)、
  Kotlin Arrow — [mapOrAccumulate](https://arrow-kt.io/learn/typed-errors/working-with-typed-errors/)
  (D3 の系譜)
- 内部: ADR-0004(context 判例)/ 0005 / 0006、使用量実測(本文)、
  `stdlib/result.almd` の partition intrinsic
