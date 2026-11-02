<!-- description: Historical interpolation desugaring decision and its scoped correctness rationale -->
<a id="stringinterp-is-not-special-syntax--desugar-it-to-concat--to_stringpart"></a>

# StringInterpは特別な構文ではない — `concat + to_string(part)`へdesugarする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](interp-is-desugar-to-tostring.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](interp-is-desugar-to-tostring.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](interp-is-desugar-to-tostring.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md)
> 原文自身が強い履歴上の正しさ主張を限定している。v0は退役、Bool/Float本体は存在し、有限byte比較は全合成の証明ではなく、desugar/formatting/ownership/rendererへの依存が残る。当時の構築指示を実行せず、旧未実装変換表を現状と扱わず保持する。scratchpad wiki参照と個人salvage pathは当時の所在であり、利用可能な成果物として検証していない。

<a id="historical-design-scope-of-the-correctness-claim"></a>

## 履歴上の設計と正しさの主張の範囲

これは2026年6月17日のdesugarの決定と取り組みの記録である。v0 oracle保持の指示と未実装変換の一覧は、現行実装向けの指示ではない。v0は退役しており、現行stdlibにはBoolとFloatの変換本体がある。

後述の「無言の誤コンパイルは構造的に不可能」という表現は、desugarが確立することを過大に述べている。遅延interpolation経路を1つ除くことで、その経路の失敗形態は除けるが、正しさは依然desugar、format本体、ownership、rendererに依存する。有限のbyte比較は証拠であり、あらゆる合成の証明ではない。[証明済みと信頼対象の区別](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)を参照。この限定を付けて、元の論拠を以下に残す。

理想の最終形（2026-06-17に設定）。位置ごとのinterp lowering方式に取って代わる。

<a id="the-principle"></a>

## 原則

`"a=${x}, b=${y}"`は意味論上、`concat("a=", to_string(x), ", b=", to_string(y))`にすぎない。
したがって正しいarchitectureは、**frontendが`StringInterp`を、各Expr partをその型の`to_string`で包んだ`ConcatStr` chainへdesugarする**こと。MIR/loweringには`StringInterp` nodeが一切見えない。するとinterpolationは、すでにself-host化・byte検証された`__str_concat`と型ごとの`to_string` primitiveを使う。`interp_str_lowerable`述語と「385/460除外」の集合は**存在しなくなる** — interpを特別なlowering構造として扱ったから存在しただけだ。

これはtrust-spineの思想そのもの、**特殊構文を証明済みprimitiveへ還元する**ことである。leaf部分（commit 83a72efa）は述語と位置別処理を加えたが、これは間違った方向（特例の増加）だった。正しい方向は特別扱いを除くことだ。

<a id="status-2026-06-17--step-2-shipped-v1-only-v0-oracle-untouched-independently-verified"></a>

## 状態2026-06-17 — step 2はv1のみ出荷（v0 oracleは無変更）、独立検証済み

このdesugarは共有frontendではなく、**v1 lowering**（`crates/almide-mir/src/lower/mod.rs`の`desugar_string_interp`）に着地した（commit f81a93a0、develop-v1）。v0リスクをゼロに保つため、frontend desugar（layer 1）は意図的に後回しにした。根拠：step 2のdiffは`almide-mir`だけ（加えて新規`stdlib/bool.almd`）に触れ、`git diff crates/almide-frontend`は空。v0の`emit_string_interp`（`emit_wasm/calls_string.rs`内、#782とともに削除 — git履歴参照）は変更しなかった。退役させたのはv1内部の`interp_str_lowerable`述語であって、v0 oracleではない。dual-oracleは完全に維持し、それがgateである：v1のdesugar出力をv0出力とbyte比較する（corpus-wall (a)=250のinterpがv0とbyte一致、さらに独立golden：Int/String/Bool/edgeはwasmtimeでbyte一致、Float/compoundは明確にwall）。+22のcaps増分は対抗的検証で健全性を確認した（Stdoutに到達するinterp operandはStdoutを推移的に表面化 → witness `|0`、declared∅ → REJECT、偽のgreenにならない）。基準値：caps 4134（ACCEPT）、mir>ir 0、FORBIDDEN 0、428 tests、3-property ACCEPT。

<a id="the-oracle-preservation-law-non-negotiable--binds-the-eventual-frontend-desugar-layer-1"></a>

### ORACLE保持の法則（交渉不可）— 最終的なfrontend desugar（layer 1）を拘束する
共有frontendへのdesugar移動だけが、v0のinterp codegenに触れる段階となる。これは延期しており、実施時は次に従う：**`emit_string_interp`を削除してはならない。**両経路を走らせ、CIで全corpusのbyteを比較し、一致を証明してから初めて`emit_string_interp`を退役させる（translation-validation、#570のpatternをdesugar自体へ適用）。「先に消してdesugarに賭ける」は禁止 — 形式的な代替を得る前にoracleを消さない。それまではv1のみのdesugarがv0リスクゼロでtrust-spineの利益をもたらす。

<a id="budget-two-independent-hard-sub-problems-dont-underestimate--clean-framework--done"></a>

### 見積り：独立した2つの難しい副問題（過小評価しない — きれいなframework≠完了）
desugar frameworkは明瞭だが、v0とのbyte一致には独立した既知の難しいformatting wallが2つある。それぞれが着地するまで、そのinterp partは**明確にwallとなる**（検証済み：Float `${f}` → `float.to_string`を名指すUnsupported）。無言の誤コンパイルにはしない。
- **`float.to_string` = dtoa（task #63）** — v0の`f64` Displayと1byte単位で一致させる必要がある：正しい最短round-trip丸め、整数値の`.0` suffix、NaN / Inf / -0.0、科学記法の閾値。（履歴：floatのnative↔wasm表示drift。）v0 formatterを厳密に再現する救出済みdtoa WIPはstep 4の本体であり、脚注ではない。
- **compound repr（`${list}` / `${record}` / `${tuple}`）= task #64** — v0の観測formatを正確に再現する必要がある：括弧、区切り、文字列要素の引用、入れ子、空のケース。self-hostした`emit_repr_value`は、もっともらしい出力ではなくv0の観測出力を写す必要がある。

<a id="what-it-reveals-the-real-work-is-total-to_string-display"></a>

## 分かること：本当の仕事は全域的な`to_string`（Display）

interp = `to_string(part)`となれば、残る仕事は**すべての型が`to_string`を持つ**ことだけだ。corpus中の385/460の「壊れた」interpは、その型の`to_string`が欠けていることだけが原因で壊れている：

| part | 必要なもの |
|---|---|
| `${n}` Int | `int.to_string` ✓（完了） |
| `${s}` String | 恒等変換 ✓ |
| `${list.len(xs)}` | 呼び出しをmaterialize → Intを`int.to_string`で包む |
| `${b}` Bool | `bool.to_string`（`if` → "true"/"false"）— 簡単、未実装 |
| `${f}` Float | `float.to_string` = **dtoa** — 難しい、未実装 |
| `${xs}` `${rec}` | `List.to_string` / record Debug-repr（`emit_repr_value`のself-host）— 未実装 |

**つまり「StringInterp」は実際には「全域的な`to_string`（Display）」の問題である。**`to_string`が全域ならinterpはどこでも追加作業なしに動く。

<a id="this-redraws-tail-vs-essential"></a>

## 「末尾作業と本質的作業」の区分を引き直す

- **`to_string` family — Int / Bool / Float(=dtoa) / List / record Display — は本質的。**interpも`print`/`println`も通り、至る所で再利用する。
- **超越関数（sin/cos/tan/log_gamma）こそ本当の末尾作業** — interpもprintもそこには到達しない。

**帰結：`float_dtoa`は末尾作業への逸脱ではなかった。**これは*`float.to_string`の前提条件*、つまり本質的なものだ。「interp = `to_string`」という捉え方がなかったため、動機のない末尾作業に見えただけである。この捉え方では**dtoaは優先すべき本質的作業**（`/Users/o6lvl4/cctmp/float_dtoa.almd.salvage`に救出済みなので復活させる）。一方trigは本当に末尾作業だった（低優先度は正しいが、実施済みであり、それは構わない）。

<a id="verification-becomes-compositional"></a>

## 検証は合成的になる

- 当時：位置ごと（bind/call-arg/tail/match-arm/concat）にinterpをbyte一致させる = 組合せ的。
- 理想：型ごとの`to_string`を1回byte一致させる → その型を使うすべてのinterpが自動的に検証される。`int.to_string`を1回証明すれば、`"${n}"`、`"x=${n}"`、`"${a+b}"`のすべてが合成的に検証される。

<a id="wall-vs-hole-dissolves-structurally"></a>

## 「wallとhole」は構造的に解消する

desugarでは、型に**`to_string`がない**partはdesugarできない → **明示的frontend error（明確なwall）**。できるpartは証明済みprimitiveを使う。**無言の誤コンパイル（およびcorpus-wallが見落とす当時のinvalid-wasm）は構造的に不可能** — 壊れたwasmをemitする「遅延Opaque interp」経路が残らないためだ。（診断では当時のnon-leaf interpがINVALID WASMを出し、明白な失敗なのにcorpus-wallが捕まえないと確認した — desugarはその経路を完全に除く。）

<a id="implementation-layers-the-care-points"></a>

## 実装のlayer（注意点）

1. **Frontend desugar**：`StringInterp{parts}` → 型ごとに`to_string(part)`を付けた`ConcatStr` fold。frontendはv0と共有するため、v0のinterp codegen経路も（`emit_string_interp`からconcat+to_stringへ）変わる。**dual-oracleで検証しなければならない**：desugar形式がすべてのpart型についてv0の当時の`emit_string_interp`出力とbyte一致すること（compound repr、Float表示のedge caseも含む）。これがgate — ここでのv0 regressionは認められない。
2. **count==lowerが追加作業なしで得られる**：desugarがfrontendにあるため、`count_ir_calls`（harness）も`lower_function`もdesugar済み`ConcatStr`+`to_string` IRを見る。特別な述語もcount対lowerの乖離もない（[[../../../.claude .../project_v1_gate_count_vs_lower]]の罠全体が消える）。
3. **v1 interp coverage == v1 `to_string` coverage。**`to_string`がv1でself-hostされているpartは動く。`to_string`がまだself-hostされていないpartは、その`to_string`呼び出しのv1 loweringが**明確なUnsupported wall**でなければならず、unlinked-call → invalid wasmにしてはならない。したがって前提条件は、**未登録／未link stdlib呼び出しのv1 lowering → `Unsupported`（wall）、決してinvalid wasmではない**こと。（あるいは型に`to_string`がなければfrontend desugar自体が拒否する。ただしv0にも拒否するため、v0がその`to_string`を持たなければv0 regressionになる。v0は全型にDisplayを持つので、desugarはv0には常に成功すべきであり、v1のwallはunlinked-call → Unsupportedである。）
4. その後**型ごとの`to_string`を埋める**（各追加がその型のinterpを合成的に自動で有効にする）：Bool（簡単）→ Float=dtoaを復活（救出済みWIP、本質的）→ compound repr（List/record Display）。

<a id="build-order"></a>

## 構築順
1. v1：未link stdlib呼び出し → `Unsupported`（構造的wallの前提）。2. Frontend desugar：StringInterp → concat+to_string、dual-oracleでv0とbyte一致（`interp_str_lowerable`と位置別処理を退役）。3. `bool.to_string`。4. `float.to_string`（dtoa）を復活。5. compound Display。
各stepの検出器は位置ごとのinterpではなく、型ごとの`to_string`のbyte一致である。
