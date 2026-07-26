<!-- description: How other languages range-check integer literals, and where Almide should land -->
# Integer literal domain — cross-language survey and Almide's target

2026-07-26 に C-173（整数リテラルの型域）を入れる過程で、`UInt64` が宣言域の
上半分で**両ターゲットとも同一に誤る**ことが判明した（#872）。「これは他言語では
どうしているのか」を実機で確かめた記録と、そこから導かれる Almide の着地点。

survey は recall ではなく**全て手元で実行**した。バージョンは
Rust 1.94.1 / Go 1.26.1 / Swift 6.1.2 / Zig 0.15.2 / .NET 10.0.105 /
Apple clang 17.0.0 / javac 26 / Python 3.11.9 / OCaml 5.4.1。

## 実測マトリクス

| 言語 | `u64 = u64::MAX` | `u64 = -5` | `u64 = -0` | `u64 = u64::MAX+1` |
|---|---|---|---|---|
| Rust | ✅ 正確 | ❌ E0600 | ❌ E0600 | ❌ out of range |
| Go | ✅ 正確 | ❌ overflows | ✅ `0` | ❌ overflows |
| Swift | ✅ 正確 | ❌ overflows into unsigned | ✅ `0` | ❌ |
| Zig | ✅ 正確 | ❌ cannot represent | ❌ **ambiguous** | ❌ |
| C# | ✅ 正確 | ❌ CS0031 | ✅ `0` | ❌ |
| C | ✅ 正確 | ⚠️ **通る**（`…611` にラップ） | ✅ `0` | — |
| Java | 型が無い | 型が無い | 型が無い | ❌ 整数が大きすぎます |
| **Almide (修正前)** | ⚠️ 通って **`0`**、`/` 後 **`-1`** | ⚠️ **通って** rustc が E0600 | ✅ `0` | ⚠️ 通って `0` |
| **Almide (C-173 後)** | ❌ E024 carrier | ❌ E024 sign | ✅ `0` | ❌ E024 magnitude |

## 軸1 — 符号なし型に負リテラル

**C 以外の全言語が拒否する。** C だけが暗黙の mod 2⁶⁴ ラップで通し、それは
`-Wsign-conversion` を付けないと警告すら出ない（付ければ出る、実測 2 件）。
これは有名な footgun であり、後発言語が揃って閉じた穴。

修正前の Almide は「C の挙動 + その先で rustc が落ちる」で、**C ですらなかった**。

拒否の理由づけには**2 つの流儀**がある。

- **演算子の話にする**（Rust）— `cannot apply unary operator '-' to type 'u64'`。
  `-5` は `Neg::neg(5)` であり `u64` に `Neg` 実装が無い、という構造的に正確な説明。
  ただし**直し方を一切教えない**。
- **値域の話にする**（Go / Swift / Zig / C#）— 型がその値を表現できない、と言う。
  Zig の `type 'u64' cannot represent integer value '-5'` が最も簡潔。

Almide は**値域の流儀**を採り、さらに**同じ幅の符号付き型を名指しする**:

```
error[E024]: integer literal '-5' is out of range for UInt64
  hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
        or use the signed Int64 if the value can go below zero
```

代替型を名指しするのは調べた範囲では**どの言語もやっていない**。mission
（LLM が最も正確に書ける言語）に照らすと、修正候補を明示するほうが
modification survival rate に効くので、ここは意図的に他言語より一歩踏み込む。

## 軸2 — キャリア問題（本丸）

**Almide の状況に前例が無い。** 理由は単純で、

> まともな言語は**リテラルの内部表現を対象型より狭くしない**。

| 戦略 | 言語 | 実測での裏づけ |
|---|---|---|
| 任意精度 | Go, Zig, Python | `(1 << 100) >> 100` が `1` を返す（機械語型を超える中間値が保たれる）。Go 仕様は実装に**整数定数を最低 256bit で表現すること**を要求している |
| 固定だが最広型以上 | Rust (`u128`), C (≥ `uintmax_t`), C#, Swift | `u64::MAX` が正確 |
| **符号付きキャリアのみ + 符号なし型を持たない** | **Java, OCaml** | 符号なし解釈を**型ではなく操作**で提供する（下記） |

Java と OCaml が重要。**キャリアが符号付きであること自体は欠陥ではない。**
両者ともキャリアで一貫しており、符号なし演算を**関数として**提供する（実測）:

```
Java    Long.toUnsignedString(-1L)          → 18446744073709551615   ✅
OCaml   Int64.unsigned_div (-1L) 2L         → 9223372036854775807    ✅
OCaml   Printf.printf "%Lu" (-1L)           → 18446744073709551615   ✅
```

OCaml はさらに徹底していて、**native `int` は 63bit**（タグ付きのため。
`Sys.int_size = 63`, `max_int = 4611686018427387903`、実測）。64bit を名乗らず、
必要なら `Int64` モジュールを明示的に使わせる。**運べない幅を宣言しない**という
態度が、まさに Almide に欠けていたもの。

つまり Almide の欠陥は「i64 キャリア」ではなく、**宣言した型とキャリアの不一致**。
`UInt64` という型を名乗りながら、その域を運べていないことが問題。
そして `uint64.to_string` が `int.from_uint64` 経由で符号付き表示していたのは、
Java/OCaml が正しく実装している「操作としての符号なし」ですらなかった。

### 帰結: 着地点は 2 つある

**(1) キャリアを広げる（Rust モデル）** — `IrExprKind::LitInt { value: i64 }` を
`i128` 相当へ広げ、MIR に `NTy::U64` と `div_u`/`rem_u`/`lt_u` を入れ、
`uint64.to_string` の i64 ピボットを外す。`UInt64` が宣言どおりになる。

**(2) 符号なし 64bit の「型」をやめる（Java / OCaml モデル）** — i64 キャリアで
一貫させ、符号なし解釈を `uint64.*` 関数として提供する。安いが、既存利用者の
型を奪う。**なお現状は (2) ですらない** — `uint64.to_string` は符号付きで表示するので、
`Long.toUnsignedString` / `Int64.unsigned_div` が満たしている水準に達していない。

**推奨は (1)。** `UInt64` という型を持つ言語は例外なく (1) を採っており、
(2) を採る Java / OCaml は**そもそも型を持たない**。型を名乗る以上、域を運ぶ責任がある。
仮に (2) を選ぶなら、`UInt64` 型の撤去まで込みでやらないと同じ嘘が残る。

### #872 への訂正

issue #872 には「MIR に `NTy::U64` を足す」と書いたが、それだけでは**直らない**。
リテラルは MIR に届く前に i64 キャリアで潰れるので、順序は

1. **リテラル表現を広げる**（ここが根本。Rust の `LitKind::Int` = `u128` と同じ）
2. MIR の符号なし演算（`div_u` / `rem_u` / `lt_u`、オペランド型による選択）
3. `uint64.to_string` / `to_float*` から i64 ピボットを除去
4. E024 の carrier キャップを撤去し、E024.md と llms.txt の `UInt64` 推奨を復活

2 だけ先にやっても 1 が無ければ意味が無い。

暫定キャップは「他言語に前例の無い異常状態」なので、**長居させない根拠**にもなる。

## 軸3 — `-0` は本当に三分する

ここだけは業界が割れており、Almide の選択には理由づけが要る。

| 挙動 | 言語 |
|---|---|
| `0` として**受理** | Go, Swift, C#, C |
| **拒否**（演算子として） | Rust |
| **拒否**（曖昧として、符号付きでも） | Zig |

Zig が独特で、`i64` に対してすら `integer literal '-0' is ambiguous` と言う
（実測）。整数の `0` と浮動小数の `-0.0` の区別がつかない、という別の理由。

Rust の拒否は**軸1で演算子の流儀を採った当然の帰結**であり、`-0` を特別扱い
しているわけではない。

**Almide は受理を維持する。** 多数派だからではなく、**軸1 で値域の流儀を採ったから**:

> E024 の規則は「その型はこの値を表現できるか」である。`UInt64` は `0` を
> 表現できる。ゆえに `-0` を拒否すると規則そのものと矛盾する。

拒否するには演算子の流儀へ乗り換える必要があり、それは軸1で捨てた
（直し方を教えないため）。**流儀の一貫性が、Rust への追随に優先する。**

一時は「例外の無い規則のほうが LLM に保持しやすい」＝拒否、とも考えたが、
これは規則の立て方を取り違えている。Almide の規則は
「符号なしに `-` は不可（例外: 0）」ではなく
「型が表現できない値は不可」であり、**例外は存在しない**。

## 軸4 — 診断の質: 値域を書く

Rust は範囲を**注記に明示する**:

```
error: literal out of range for `u64`
  = note: the literal `18446744073709551616` does not fit into the type `u64`
          whose range is `0..=18446744073709551615`
```

Almide の magnitude ヒントは "use a literal within the type's range" とだけ言い、
**その range が何かを言っていない**。`UInt32` の上限を諳んじられる読み手（人間でも
LLM でも）は多くない。範囲を書くのは安く、mission に直結する。

→ 実施済み。E024 の magnitude ヒントは具体的な範囲を出す。

## Almide がこの survey から採った / 採らなかったもの

| 項目 | 採否 | 出典 |
|---|---|---|
| 符号なしへの負リテラルを拒否 | 採用 | Rust/Go/Swift/Zig/C# 全員 |
| 値域の流儀で説明する | 採用 | Go/Swift/Zig/C#（Rust の演算子流儀は不採用） |
| 代替の符号付き型を名指し | **独自** | どの言語もやっていない |
| ヒントに具体的な値域を書く | 採用 | Rust |
| `-0` を受理 | 採用 | Go/Swift/C#（Rust/Zig は不採用、理由は軸3） |
| キャリアを最広型以上に広げる | **未了 → #872** | Rust/Go/Zig/C/C#/Swift 全員 |
| 符号なし型を持たず関数で提供 | 不採用 | Java（型を名乗る以上、域を運ぶ） |

## 付録 — 各言語の実測メッセージ全文

診断を設計するときの一次資料。全て上記バージョンで手元実行したもの。

### 符号なし型に負リテラル（`u64 = -5`）

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Go     cannot use -5 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: negative integer '-5' overflows when stored into unsigned type 'UInt64'
Zig    error: type 'u64' cannot represent integer value '-5'
C#     error CS0031: Constant value '-5' cannot be converted to a 'ulong'
C      （通る。-Wsign-conversion で警告 2 件。値は 18446744073709551611）
Almide error[E024]: integer literal '-5' is out of range for UInt64
       hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
             or use the signed Int64 if the value can go below zero
```

Zig が最も簡潔で、Swift が最も説明的。**修正候補の型を出すのは Almide だけ。**

### 型の域を超える大きさ（`u64 = u64::MAX + 1`）

```
Rust   error: literal out of range for `u64`
       = note: the literal `18446744073709551616` does not fit into the type `u64`
               whose range is `0..=18446744073709551615`
       = note: `#[deny(overflowing_literals)]` on by default
Go     cannot use 18446744073709551616 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: integer literal '18446744073709551616' overflows when stored into 'UInt64'
Zig    error: type 'u64' cannot represent integer value '18446744073709551616'
C#     error CS1021: Integral constant is too large
Java   エラー: 整数が大きすぎます。      ← long のみ。符号なし型が無い
Almide error[E024]: integer literal '18446744073709551616' is out of range for UInt64
       hint: UInt64 would silently fold to 0 here; its range is 0..=9223372036854775807,
             so use a literal within it, or model larger magnitudes as Float (lossy)
             or a parsed string
```

**範囲を明示するのは Rust と Almide だけ。** 他は「大きすぎる」で止まる。
Almide が出す `UInt64` の範囲は宣言域ではなく**実際に通る範囲**（#872 のキャップ後）。
コンパイラが守らない範囲を診断に書くのは、範囲を書かないより悪い。

### `-0`

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Zig    error: integer literal '-0' is ambiguous        ← i64 に対しても同じ
Go     0
Swift  0
C#     0
Almide 0
```

## 関連

- 契約 C-173 — `docs/contracts/contracts.toml`、規範は ALS-M14
- #872 — `UInt64` の上半分（本文書の軸2 が直し方の順序を決めている）
- #873 — レクサーの radix リテラル（`0x` 単体が黙って 0、`0b`/`0o` が未対応）
- `docs/diagnostics/E024.md` — 3 つの逸脱と 3 つのヒント
- `tests/int_literal_domain_test.rs` — 分類の固定
