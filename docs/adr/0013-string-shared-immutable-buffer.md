# ADR-0013: `String` becomes a shared immutable buffer — the codec ceiling is a representation, not a law

- **Status**: Accepted（2026-09-04 ○。初稿「String は所有のまま、上限を台帳へ」は ../almide-references の
  再調査で ×: 上位言語は全員「不変の共有バッファ」で、所有コピーを既定にしている言語は無い。本稿は
  Alternative A を採用した書き直し）
- **Date**: 2026-08-30（初稿）/ 2026-09-04（改訂・批准）
- **決定範囲**: Almide の `String` 値の native **表現**（所有 `String` → 参照カウント共有の不変バッファ）と、
  `mut` 面（`string.push` / `string.clear`）の copy-on-write。`Value` の表現は含まない（#1679、別アーク）。
- **関連**: #1673 / #1678 / #1679 / #1004、`docs/design/MEMORY-SAFETY.md`（RcCow）、ADR-0004（stdlib の String 終着）、
  C-132（mut 引数の書き戻し規約）。

## Context

「上流のスキーマ検証ライブラリ（`../almide-references` 配下、v4.5.2）に負けない」を目標に、
同一ワークロード（8 フィールド + ネスト record + `List[String]` の record を 100 万回、arm64、ns/op）で
詰められる機構を全部詰めた結果:

| | v0.59.2 | develop (2026-08-30) | 上流 |
|---|---:|---:|---:|
| `for v in data { User.decode(v) }` | 1693 | **205** | `safeParse` ~100 |
| `json.parse`（同一文書） | 948 | **515** | `JSON.parse` ~500 |
| parse + decode（end to end） | 1576 | **735** | ~600 |

残差は一つに絞れている。`User.decode` の 205 ns のうち約 160 ns は record の `String` フィールド 8 本の
**所有コピー**で、上流は払わない（JS の文字列は共有される）。同じ形を Rust で手書きすると、フィールドを
借りる record は 53 ns、所有 `String` の record は 168 ns（#1679）。

同じ根が別のベンチにも出ている。strchurn（#1004）の差の **3/4** は `string.split` が `Vec<String>` を返す
こと — Almide に借用文字列型が無いので、部分文字列は必ずコピーになる
（`research/benchmark/perf/string-gap-1004.md`）。

### 参照コンパイラはどうしているか（../almide-references、2026-09-04 再調査）

| 言語 | `String` 表現 | 一次資料 |
|---|---|---|
| Koka（本設計の Perceus の直系） | 不変・参照カウント共有。4 表現: 空 singleton / ≤7 B inline 小文字列 / 通常 / raw buffer。カウントは**非アトミック**、スレッド共有された値だけ負数カウントでアトミックに切り替える | `koka/kklib/include/kklib/string.h:13-22`、`kklib.h:103-114` |
| Swift | copy-on-write + 15 B 小文字列 inline、リテラルは immortal（ARC 省略） | `swift/stdlib/public/core/StringObject.swift:28-45` |
| Roc | 小文字列 inline + 参照カウント、seamless slice でバッファ共有 | （tree は浅く一次資料なし。周知の設計） |
| Gleam / Erlang | 不変 binary を共有、sub-binary で部分列共有 | — |
| Rust / Zig | owned / borrowed の二本立て | — |

所有コピーを既定にしている上位言語は無い。Rust は owned だが `&str` があり、Almide に無いのがまさに
#1004 の差である。

### Almide 側の事実

- **wasm レッグは既に共有表現**: String は `rc@0` を持つ参照カウント付きブロック（`crates/almide-layout`）。
  native だけが所有コピー。「表現を変えると byte-identity の証明対象が増える」は逆で、二本のレッグを
  揃える方向になる（初稿の撤回条件「wasm が共有表現になったら」は既に成立していた）。
- **破壊的操作は 2 つだけ**: `string.push(mut s)` と `string.clear(mut s)`。List / Map が既に使っている
  RcCow（唯一なら in-place、共有なら複製）をそのまま適用でき、値意味論は変わらない。
- **初稿が「遅い」と測った試作は別物**: `Value` 側の `Arc<str>` + HashSet intern（ハッシュ探索込み、
  parse +6% / decode +20%）。単純な共有（clone = カウント +1、~1-5 ns）と、確保 + memcpy（~20 ns/本）の
  比較ではない。

## Decision

**native の `String` は参照カウント共有の不変バッファになる。** `mut` 面（`push` / `clear`）は RcCow で
唯一性を見て in-place / 複製を選ぶ。部分文字列（`split` / `slice` / `trim` …）は同じバッファを指す
seamless slice で返す。wasm レッグの表現（rc ブロック）とは観測上等価。

段階（各段が単独で着地し、ratchet が前段を守る）:

1. **ratchet を先に置く**: strchurn / fasta / decode（`User.decode` 100 万回）の 3 本を
   `research/benchmark/perf/` の A/B 台帳に載せ、`scripts/check-perf-ratio.sh` の対象にする。
2. **表現の切替**: 生成 Rust と `runtime/rs` の `String` を `AlmideStr`（`Rc<str>` 系、非アトミック）に。
   `push` / `clear` は RcCow。fan 越境は Koka の thread-shared 方式（fan に渡した値だけアトミック化）
   — もし段階 2 の計測でアトミック RC が strchurn を悪化させるなら、ここで止めて表現を戻す
   （ratchet が赤 = 着地しない）。
3. **seamless slice**: `split` / `slice` / `trim` / `lines` 系がバッファを共有する。#1004 の 3/4 はここで消える。
4. **小文字列 inline**（Koka ≤7 B / Swift 15 B）は別 ADR — ABI が変わり wasm と乖離するため本稿の範囲外。

実装は 0.62.0 の**後**（表現変更はリリース境界をまたがない）。

## Rationale

1. **値意味論は COW が守る。** プログラムが観測できる結果は所有コピーと同一。RcCow は List / Map で
   既に稼働しており、String に広げるのは新機構ではない。LLM の書き手が知る必要のある規則は増えない。
2. **二本のレッグが同じ形になる。** wasm は既に共有。native を揃えることで、C-132 の mut 書き戻しや
   alias_safety の議論が「両レッグ同じ表現」の上で出来る。
3. **差は上限ではなく表現だった。** 「decode ≒ 100 ns + 20 ns × String フィールド数」は所有コピーの
   線形則であり、共有バッファでは ~1-5 ns × 本数に落ちる（手書き 53 ns が下界の証拠）。

## Alternatives

- **A'. 所有のまま、上限を台帳へ（初稿）。** ×。上位言語の設計と、wasm レッグの既存表現の両方に反する。
- **B. デコード側の借用 record。** 言語に参照型が無いので表現不能。却下。
- **C. `Value` の共有化。** 実測で遅い（#1679）。却下（別アーク）。
- **D. SSO のみ。** 確保は消えるがコピーと `split` の問題が残る。段階 4 として本決定の後に検討。

## Consequences

- 良くなる: decode の String フィールドコスト ~20 ns → ~1-5 ns/本、strchurn の split コピーが消える、
  両レッグの String 表現が揃う。
- 払う: `runtime/rs` と生成 Rust の String 型が変わる（ADR-0004 の終着面は API であり表現ではないので
  凍結面には触れない）。`push` / `clear` に唯一性分岐が入る。fan 越境のアトミック化コストは段階 2 の
  ratchet で計測してから受け入れる。
- 変わらない: 契約（observable は同一）、`String` の API、wasm レッグ。

## Falsifier

段階 2 の ratchet で strchurn / fasta のどちらかが **5% 以上悪化**し、thread-shared 方式でも回復しないなら、
段階 2 を revert して本 ADR を Superseded にし、A' に戻す。

## References

- #1673, #1678, #1679, #1004、PR #1680–#1687
- `../almide-references/RESEARCH.md` §5（JSON 文字列走査）、§6（Koka の借用/所有環境）
- `../almide-references/koka/kklib/include/kklib/string.h`、`kklib.h`（表現と thread-shared RC）
- `../almide-references/swift/stdlib/public/core/StringObject.swift`
- `research/benchmark/perf/string-gap-1004.md`
