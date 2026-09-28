<!-- description: 関数ごとの law を Lean をカーネルにして証明する層の設計。純粋な断片の自動翻訳、proved / tested / refuted の算出、Int の折り返しと剰余の意味、停止性、翻訳の信頼の鎖、Lean の利用条件(ライセンス・商標)、段階計画。 -->
# law 証明層 — 設計 (draft, 2026-09-28)

> **これは何か**: [behavioral-contract](../active/behavioral-contract.md) の **C-ASSERTED**
> (機械が短い性質を提案し、人間は文面だけを批准する)を、**Lean をカーネルにした証明**で裏打ちする
> 層の設計。実測の根拠は [law-blocks-experiment](law-blocks-experiment.md)。
>
> **一文で**: ユーザーは関数ごとに `law`(全入力で成り立つ Bool 式の性質)を書く。コンパイラは
> 純粋な関数とその law を自動で Lean に翻訳し、呼び出し関係の下から上へ固定の機械手順で証明を
> 試みる。閉じた law には **Lean と同じ強さの保証** が付き、閉じなかった law は生成入力による
> 検査(`tested`)に落ちる。反例が見つかった law は **コンパイルエラー** になる。
>
> **状態**: 設計のみ。実装は未着手。§9 の段階 0 から始める。

---

## 0. 先行例との関係と、主張の範囲

この設計の中核 ――「利用者は補題(law)の **文** を足し、証明は機械が帰納法と書き換えで探す」――
は新しい考え方ではない。

- **Boyer–Moore の証明器 / ACL2**(1970 年代〜): 利用者の仕事は、証明器が詰まったところに補題の文を
  足すこと、証明の探索は機械が帰納法と書き換えで行う ―― という方法論そのもの。実験 3(関数ごとの
  law で 7/16)は、これを Almide の文脈で追試した結果にあたる。
- **関数ごとの契約を呼び出し関係に沿って組み上げる** 形は、Dafny / Why3 / Liquid Haskell の標準。
- **証明 = プログラム、帰納法 = 再帰** は、カリー=ハワード対応(型理論の基礎)。
- **LLM に仕様・補題を書かせて証明器で確かめる** 研究も、2024〜25 年に Dafny / Verus を対象に複数ある。

この設計が新しく持ち込むのは、(1) 書き手が **LLM** である前提で、law の文だけを書かせ、判定レベルを
算出し、反例を先に出すことを **言語の表面** に組み込むこと、(2) それを **MSR** で測ることの二点で、
どちらもまだ小さな実験(6 課題・1 モデル)の段階にある。

**範囲の限定**: 「証明はプログラムと同じ形をしている」という見方が効くのは、リストや自然数のように
**帰納的に作られるデータについてのプログラムの証明** に限る。排中律・選択公理に頼る証明(対応する
プログラムがない)や、実数・連続性のような無限の対象についての証明は、この層の対象外であり、
数学全般についての主張ではない。


「Lean 相当」には三つの意味があり、この層が狙うのは一つだけ([law-blocks-experiment](law-blocks-experiment.md) §5)。

| 意味 | この層 |
|---|---|
| **保証の強さ** ―― Almide のコードについて、カーネル検査済みの証明がある | **狙う**(純粋な断片に限る) |
| 表現力 ―― Lean で証明できることは何でも証明できる | 狙わない(依存型理論と Mathlib の再現は Lean の作り直し) |
| 自動性 ―― law を書けば必ず証明が見つかる | 原理的に不可能。天井を上げ続けるだけ |

**目標**

- G1. ユーザーが書くのは law の **文** だけ。タクティク・補題名・証明項は書かない(LLM にも書かせない)。
- G2. `proved` と表示された law は、**その Almide のコードについて** 成り立つ ―― 翻訳の誤りで
  偽の `proved` を出さない(§5)。
- G3. 証明の構造がコードの呼び出し構造と一致する。失敗時は「どの関数の law が足りないか」を指せる。
- G4. 判定は宣言でなく **算出**。ソースに「これは証明済み」と書く欄を作らない。

**非目標**

- `var`、effect fn、IO、クロージャの捕獲を含むコードの証明(段階 3 以降で再検討)。
- Float の性質の証明(IEEE 754 のモデル化は別の研究)。
- 手書き証明の言語。閉じない law の逃げ道は `tested` であって、人間が証明を書く口ではない。
- 独自の証明カーネル。最終検査は Lean に任せる。

## 2. 利用者から見た姿

```almide
fn rest_chunks(xs: List[Int], n: Int) -> List[List[Int]] =
  if list.len(xs) == 0 or n <= 0 then []
  else [list.take(xs, n)] + rest_chunks(list.drop(xs, n), n)

law "rest_chunks rebuilds its input" for (xs: List[Int], n: Int where it >= 1) {
  list.flatten(rest_chunks(xs, n)) == xs
}

law "rest_chunks makes no empty chunk" for (xs: List[Int], n: Int) {
  rest_chunks(xs, n) |> list.all((c) => list.len(c) > 0)
}
```

- `law` は関数と同じ位置に書くトップレベル宣言。本体は **Bool 式**。
- `for (...)` は全称の引数。`where` は引数の事前条件(決定可能な述語だけ)。
- どの関数の law かは、本体が呼ぶ関数から推定し、明示したいときは `law rest_chunks "..."` と書ける
  (§4.3 の証明順序に使う)。

`almide check --laws` の出力(例):

```
proved   rest_chunks rebuilds its input          (by recursion on rest_chunks)
proved   rest_chunks makes no empty chunk        (by recursion on rest_chunks)
proved   chunk rebuilds its input                (from: rest_chunks rebuilds its input)
tested   only the first chunk may be short       (1000 cases, no counterexample)
         not proved: needs arithmetic with a variable divisor (len % n); see §6.1
error[E1xx]: law "unique keeps length" is false
  counterexample (shrunk in 7 steps):
    xs = [0, 0]
```

## 3. 判定のレベル

| レベル | 意味 | 算出の条件 |
|---|---|---|
| `refuted` | 偽 ―― **コンパイルエラー** | 生成入力で反例が見つかった |
| `proved` | 全入力で成り立つ(Lean のカーネル検査済み、§5 の鎖つき) | 翻訳 + 証明が閉じた |
| `tested` | 反例は見つからないが、示せていない | 上のどちらでもない |
| `trusted` | 人が責任を持つと宣言 | **ソース外** の台帳(`proofs/law-trust.toml` 等)にだけ書ける |

原則は二つ。**誤りの証拠があるときだけ落とす**(示せないだけなら落とさずレベルを下げる)。
**レベルは宣言でなく算出**(ソースに `@proved` を書く欄を作らない ―― LLM が逃げ込む注釈を作らない)。

`tested` → `proved` の格上げ、`proved` → `tested` への格下げ(コード変更で証明が再発見できなかった)
は、どちらも差分として報告する。CI では「`proved` の数を減らさない」ラチェットにできる。

## 4. アーキテクチャ

```
.almd ─ parse ─ check ─┬─ law の型検査(Bool 式・決定可能な where)
                       │
                       ├─ [tested] 生成器 + 縮小(seed 固定)で反例探索 ── 反例 → refuted(エラー)
                       │
                       └─ [proved] 純粋断片の抽出 → Lean 翻訳 → 証明手順 → lean(外部プロセス)
                                     │                                   │
                                     └── キャッシュ(IR ハッシュ + law 文 + 手順版)←┘
```

### 4.1 生成と反例探索(`tested` / `refuted`)

スパイクの `research/spike/law-blocks/law.almd` をコンパイラ内蔵にしたもの。

- 生成器は **型から導出**(Int / Bool / String / List / Option / Result / レコード / バリアント)。
- `where` は生成器の制約として解釈(`it >= 1` → 下限つき生成)。解釈できない述語は棄却サンプリング。
- 縮小は **0 / 空へ単調** でなければならない(スパイクで循環を踏んだ)。
- seed はファイルパスと law 名から決定的に導出 ―― 同じ law は毎回同じ入力で検査される。

### 4.2 翻訳(Almide IR → Lean)

対象は **純粋な断片**(§4.4)の関数と、その law。翻訳は **ソースでなく型付き IR から** 行う
(`IrProgram` はすべての節点が型を持つ)。

- 関数 → Lean の `def`。再帰は Lean の再帰に、`match` は `match` に。
- レコード → `structure`、バリアント → `inductive`、`Option` / `Result` → Lean の同名型。
- law → `theorem`。`for` の引数は全称束縛、`where` は仮定、本体の Bool 式は `= true` の命題。
- 標準ライブラリ関数 → 「Lean モデル」への対応表(§6.2)。

数の意味の扱いは §5.2 で決める。

### 4.3 証明手順(固定)

実験 3 の規律をそのまま手順にする。

1. law を **対象関数** ごとにまとめ、呼び出し関係の逆位相順(下から上)に並べる。
2. 各 law に、同じ固定手順を適用する:
   `grind [対象関数の定義, 証明済みの law, 標準ライブラリの law]` →
   `simp_all [...]` → 対象関数の再帰に沿った `fun_induction` → リスト引数の `induction`。
3. 証明できた law だけを、上の段で使える(未証明の law を前提にしない)。
4. 閉じなかったら、失敗の形から **足りない law の場所** を推定して報告する
   (例: 帰納法の途中で呼び出し先 `f` の結果について何も言えずに止まった → 「`f` の law が足りない」)。

手順の中身(タクティク名・順序・時間上限)は **手順版** として版管理し、キャッシュのキーに含める。
手順を変えたら全 law を再判定する。

### 4.4 純粋な断片(段階 1 の範囲)

| 入る | 入らない |
|---|---|
| `fn`(effect でない)、自己再帰・相互再帰 | `effect fn`、IO、`!` |
| `Int` / `Bool` / `String` / `List` / `Option` / `Result` / レコード / バリアント | `Float`、`Bytes`、`Map` / `Set`(段階 2) |
| `match` / `if` / `let` / パイプ / ラムダ(捕獲なし) | `var`、`for`、`while`(段階 3) |
| 標準ライブラリのうち Lean モデルがあるもの | Lean モデルのない標準関数 |

断片の外の関数を呼ぶ law は、自動的に `tested` 止まりになる(`proved` を試みない)。

### 4.5 キャッシュ

証明は派生物としてキャッシュし、ソースには置かない。キーは
「対象関数とその推移的な呼び出し先の IR ハッシュ + law の文 + 手順版 + Lean の版」。
Almide のビルドキャッシュが IR キーであるのと同じ考え方で、改名やコメント変更では再証明しない。

## 5. 信頼の鎖 ―― 偽の `proved` を出さないために

実験の証明は「手で写したモデルについての証明」だった([law-blocks-experiment](law-blocks-experiment.md) §5)。
この層の中心課題は、その鎖を締めること。

```
Almide の関数 ─①翻訳─ Lean の def ─②文の翻訳─ Lean の theorem ─③証明─ カーネル検査
```

### 5.1 各輪の締め方

| 輪 | 段階 1(安い) | 段階 2 以降(強い) |
|---|---|---|
| ① 関数の翻訳 | **差分検査**: law の生成器の入力(数百件)を、Almide の実行と Lean の `#eval` の両方に通して一致を見る。不一致なら `proved` を出さない | 翻訳の正しさを、`proofs/ALS.v` と同系統の意味論に対して翻訳検証する |
| ② 文の翻訳 | **実行照合**: Lean の定理文を決定可能な Bool として同じ入力で評価し、Almide 側の law の判定と一致を見る | ①と同じ枠組みに入れる |
| ③ 証明 | Lean のカーネル | 同左。`#print axioms` で `sorryAx` が無いことを毎回確認 |

段階 1 では ①② は **テスト級** の信頼にとどまる。そのため段階 1 の `proved` は
「**翻訳が差分検査を通った範囲で**、カーネル検査済み」と正直に表示する(behavioral-contract §5:
どの出所のどの主張かを明記する)。

### 5.2 数の意味 ―― 実測で確認した二つの落とし穴

Almide 0.64.0 で実測(2026-09-28):

| 式 | Almide | Lean の `Int` |
|---|---|---|
| `int.max_value() + 1` | `-9223372036854775808`(**黙って折り返す**) | 無限精度なので折り返さない |
| `(0 - 7) % 3` | `-1`(**切り捨て除算の剰余**) | `2`(非負の剰余、`Int.emod`) |

素朴に `Int` を Lean の `Int` に写すと、**Lean では真だが Almide では偽** の law に `proved` が付く
(例: 「`x + 1 > x`」は Lean の `Int` では真、Almide では `max_value` で偽)。これは G2 の違反。

**方針**:

- `%` と `/` は Lean の **切り捨て版**(`Int.tmod` / `Int.tdiv`)に写す。取り違えを許さない。
  確認済み: `Int.tmod (-7) 3 = -1`、`Int.tdiv (-7) 3 = -2` は Almide の `(0 - 7) % 3`、`(0 - 7) / 3` と一致し、
  素の `(-7 : Int) % 3` は `2` で一致しない(Lean 4.29.1)。
- `Int` は二層で扱う。
  1. **証明はまず無限精度の `Int` で試みる**(自動化が強い)。
  2. 同時に、翻訳した関数の算術ごとに「**この演算は溢れない**」という付帯条件を生成し、それも証明する。
  3. 付帯条件がすべて閉じたときだけ `proved`。閉じなければ `tested` に落とす。
- 代替案として、`Int` を Lean の `Int64`(`BitVec 64` 上の折り返し算術)で写し、`bv_decide`(SAT、
  コアの `Std.Tactic.BVDecide`。Mathlib 不要)を使う道もある。`Int64` の `max + 1` は Almide と同じ
  `-9223372036854775808` になることを確認済み。固定幅の算術はこちらが強いが、リストの帰納法との組み合わせは未知数。段階 1 で
  両方を小さく比べてから決める(未決事項 Q1)。

### 5.3 停止性

Lean は停止しない定義を受け付けない。翻訳した関数が Lean の停止性検査を通らなければ、
その関数に依存する law は `proved` にならない。

- 構造的再帰(リストの尾で再帰する等)は自動で通る。
- それ以外は、Lean の停止性の自動推定に任せ、通らなければ診断で「何が減るか」を尋ねる。
  ユーザーが書けるのは `shrinks len(xs)` のような一語の宣言だけで、証明は機械がする。
- 実験では、`chunk` が `n <= 0` で無限ループすることを、この検査が **定義の時点で** 見つけた
  (テストも law も捕まえていなかった)。停止性の失敗は、それ自体を警告として出す価値がある。

## 6. 天井を上げる仕組み

実験 3 で閉じなかった 9 本は、算術・畳み込み・数の書式の三種類だった。

### 6.1 算術 ―― 標準関数に閉じ込める

変数による割り算・剰余・掛け算(非線形整数算術)は、一般には決定不能。ユーザーのコードに
書かせない方向で避ける。

- `rotate(xs, k)`、`chunks_of(xs, n)`、`page(xs, i, size)` のような **law 付きの標準関数** を用意し、
  それぞれの law は標準ライブラリ側で(人手を含めて)一度だけ証明しておく。
- ユーザーの law は、その標準関数の law を使って組み上がる(剰余の算術に触れずに済む)。
- `%` や `*` を変数同士で直接使った関数の law が閉じないとき、診断で該当する標準関数を示す。

### 6.2 標準ライブラリの law

標準関数は **Lean モデル + law + その証明** の三点セットで提供する。

- Lean モデル: 標準関数の意味を Lean で書いたもの(§5 の差分検査の対象)。
- law: 利用者の証明で使える性質(`list.take(xs, n) + list.drop(xs, n) == xs` など)。
- これは「ドキュメントの一部として読める law」でもある(`docs/stdlib/` に載せる)。

実験では、固定リストに一般的な law が 1 本欠けているだけで閉じない例が 2 件あった
(`Pairwise.imp`、`pairwise_cons`)。標準ライブラリの law の網羅は、天井に直接効く。
API family の網羅と同じく、**行列で網羅を検査するゲート** を置く(CLAUDE.md の API family 規則)。

### 6.3 畳み込み ―― 不変条件の候補を機械が出す

`foldl` の途中の累積値についての不変条件は、コードのどこにも書かれていない。選択肢:

1. ユーザーに累積値の law を書かせる(`law for fold in dedup ...` のような形)。
2. 畳み込みを、law 付きの標準形(`list.dedup_sorted` など)に寄せる。
3. **候補を機械が出す**: 実行記録(生成器の入力で畳み込みを回した各ステップの状態)から不変条件の
   候補を学習し、証明器が確かめる。`almide-graphics/nn` の微分可能な論理ゲートネットワーク
   (学習後に論理回路として取り出せる)が、原子的な述語の組み合わせの学習に使える見込み。
   候補が間違っていても証明器が落とすだけなので、健全性は損なわない。

段階 2 で 3 を小さく試す(`dedup` の「狭義単調増加」を閉じられるか)。

## 7. Lean の利用条件

2026-09-28 に一次資料で確認した。

### 7.1 ライセンス

- Lean 4 本体: **Apache License 2.0**([LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE))。
- Mathlib: Apache License 2.0(この層は Mathlib に依存しない前提)。
- Lean の配布物に含まれる第三者成分([LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES)):
  LLVM(Apache 2.0 with LLVM Exceptions)、**GNU C Library(LGPL 2.1)**、**GNU MP(LGPL 3)**、CaDiCaL(MIT)ほか。

この層の使い方に当てはめると:

| 使い方 | 条件 |
|---|---|
| Almide が Lean を **外部プロセスとして呼ぶ**(利用者が elan で公式ツールチェインを入れる) | Apache 2.0 の範囲で問題なし。Almide 側に再配布の義務は生じない |
| Almide が生成した `.lean` ファイル | Almide の出力であり、Lean のコードではない。Almide 側のライセンスで扱える |
| Almide の配布物に **Lean のツールチェインを同梱** する | Apache 2.0 の告知義務(LICENSE / NOTICE の同梱)に加え、同梱物に含まれる **LGPL 成分(glibc、GMP)の義務**(ソースの入手手段の提示など)が生じる |

**方針**: 段階 1〜2 は **同梱しない**。利用者に elan で公式ツールチェインを入れてもらい、版は
`lean-toolchain` ファイルで固定する(リポジトリ内の既存の Lean プロジェクトは `leanprover/lean4:v4.29.1`)。
同梱が必要になったら、その時点で LGPL 成分の扱いを別途確認する。

### 7.2 商標

「Lean」の名称とロゴは Lean FRO の商標([Lean Trademark Policy](https://lean-lang.org/trademark-policy/))。

- **許可なく使える**: 「Lean programming language and proof assistant と互換」「Lean を使う」のように、
  ソフトウェアとの関係を **正確に述べる** こと(商用・非商用とも)。文書・論文での言及。
- **許可が要る**: 他の商標の中に Lean を含めること、Lean のロゴからの派生ロゴ、大きく改変した版を
  Lean の名で配布すること。
- 使い方は **形容詞として、一般名詞を伴って**(「Lean proof assistant」)。

**方針**: 機能名・コマンド名・ドキュメントの見出しに「Lean」を **製品名として** 入れない
(× 「Almide Lean」「Lean for programmers」)。説明文で「証明の検査に Lean proof assistant を使う」と
述べるのは問題ない。機能名は `law` / `almide check --laws` のように Lean を含まない名前にする。

(本節は一次資料の要約で、法的助言ではない。同梱や商用の配布形態を決める段階で、改めて確認する。)

## 8. MSR と LLM の観点

- LLM が書くのは law の文だけ(G1)。補題名のハルシネーションと、修正に弱い手書き証明を
  最初から排除する。
- 反例は修正ループに直接効く形で出す(縮小済み・引数名つき)。
- 実験 1 の発見: 弱いテストで判定する MSR は約 2 割水増しされていた。law の判定(`refuted`)は
  その水増しを削る側に働くので、MSR の数値は下がって見えうる。導入時は「生き残ったが実は誤り」の
  率を並べて報告する。
- law そのものの書き換え(反例を消すために law を弱める)は、コードの変更と区別して差分に出す。
- 未検証の前提: **証明が閉じる形の law(特に内側の関数の law)を LLM が書けるか**。段階 1 の
  出口条件に含める(§9)。

## 9. 段階計画

| 段階 | 中身 | 出口条件(測るもの) |
|---|---|---|
| 0. 足場 | `law` 構文、型検査、生成器と縮小をコンパイラ内蔵に。`tested` / `refuted` のみ | spec のテストで `refuted` が縮小済み反例を出す。MSR の軽量計測(Grammar Lab `law-spec`)を develop のコンパイラで再走 |
| 1. 翻訳 | 純粋な断片の IR → Lean 翻訳、差分検査(①)と実行照合(②)、固定手順、キャッシュ | 実験の 6 課題を **自動翻訳で** 再現し、手写し版と同じ 7/16 が `proved` になる。差分検査が意図的に壊した翻訳を捕まえる |
| 1.5 LLM | law の文を LLM に書かせる | LLM が書いた関数ごとの law で、何本が `proved` まで届くか(実験者が書いた場合の 7/16 と比べる) |
| 2. 天井 | 標準ライブラリの law 付き関数(§6.1–6.2)、`Int` の付帯条件(§5.2)、畳み込みの候補生成の試行(§6.3) | 算術の 3 本(chunk / rotate / paginate)が標準関数経由で閉じる。`dedup` の不変条件が候補生成で見つかる |
| 3. 拡張 | `var` / `for` を含む関数、`Map` / `Set`、翻訳の形式的検証 | 別途設計 |

## 10. 未決事項

- **Q1**: `Int` を無限精度 + 付帯条件で扱うか、`Int64`(`BitVec`)で扱うか(§5.2)。段階 1 で比較。
- **Q2**: `law` の配置 ―― 関数の直後に書くか、`*_laws.almd` のような別ファイルか。LLM の書きやすさと
  修正の局所性(edit-locality)の両面で測る。
- **Q3**: `where` の述語の範囲 ―― 決定可能な断片(線形算術と長さ)に限るか。
  [compile-time-contracts](compile-time-contracts.md) の `where` と文法を共有するか。
- **Q4**: Lean を持たない環境(CI の一部、playground、wasm)での振る舞い ―― `tested` まで落として
  続行するか、`proved` の再現を必須にするか。
- **Q5**: 手順版を上げたときに `proved` が `tested` に下がる law が出たら、それを退行として扱うか。
- **Q6**: ALS 台帳(als リポジトリ)との関係 ―― law の判定レベルを契約台帳の証拠クラスに載せるか。

## 11. 参照

- 実測: [law-blocks-experiment](law-blocks-experiment.md)(MSR 実験・Lean 自動証明・関数ごとの law)
- 上位設計: [behavioral-contract](../active/behavioral-contract.md)(C-ASSERTED)
- 事前条件: [compile-time-contracts](compile-time-contracts.md)
- 所有権断片の意味論と翻訳検証: `proofs/ALS.v`
- 先行例: Boyer–Moore / ACL2(補題の文を足し、証明は機械が探す方法論)、Dafny / Why3 / Liquid Haskell
  (関数ごとの契約のモジュール検証)、Aeneas(Rust → Lean)、hs-to-coq(Haskell → Coq)、
  CLN2INV(連続緩和した論理によるループ不変条件の学習)
- Lean: [LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE)、
  [LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES)、
  [Trademark Policy](https://lean-lang.org/trademark-policy/)
