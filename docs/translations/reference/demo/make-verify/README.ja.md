<a id="make-verify--the-trust-demo"></a>

# make-verify — 信頼のデモ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文：[demo/make-verify/README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/demo/make-verify/README.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。読者向けの非規範的なスナップショット訳であり、自動同期されない。原文の時点・適用範囲・主張を維持したもので、翻訳時に独立した実測や実装検証は行っていない。 原文の約 100%・8 件中の検出数・「最もよくある」「一段階で修正」といった表現は、原文が報告する限定的実験・説明の記録として残す。本訳で再実行せず、一般的な AI 正答率や Python 全体との優劣、すべての論理ミスの検出可能性を独立に立証していない。

**主張は「AI は正しいコードを書く」ではない。主張するのは、AI のコードが_誤っている_とき（このデモではそのような間違いを複数試す）、Almide は失敗を_明確_かつ_修復可能_にできるということだ。別個の証明書検査は、文書化した適用範囲内の特定のモデル化された性質を確立するものであり、コンパイルできるコードがすべて正しい、または完全に安全だと確立するものではない。**

例が比較するのは具体的なコンパイラ診断と Python の `py_compile` ゲートであり、他言語で利用できるすべての道具や安全技法ではない。

<a id="why-this-framing-not-modification-survival-rate"></a>

## この捉え方にする理由（「modification survival rate」ではなく）

modification-survival を直接計測した（LLM が動いているコードを変更しても、引き続き合格するか）。強いモデル（Claude 級）で中程度のタスクなら、**すべての**言語で約 100% になる。これは天井効果であり、有能なモデルはそもそも間違いをしない。それは測る対象として適切でない。

本当の差は**間違いが_あった_ときに何が起こるか**にある。そして：

- **モデル群は Claude だけではない。** エージェント群は、より安価/小型/オープン/古いモデルも動かし、はるかに大きな変更で、はるかに頻繁に間違える。
- したがって、モデルに依存しない正直な計測は、**間違いを注入**し、モデルではなく_言語_に「失敗は見えて修正できるか、それとも黙って通るか」と問うことである。

この応答は**言語 + コンパイラ + proof spine**の性質であり、モデルが強くなっても消えない。

<a id="the-reported-eight-case-experiment"></a>

## 報告された八ケースの実験

元のデモ実験では、**AI が作成した変更の間違いを 8 件**注入し（各件は別々のエージェントが Almide と Python の両方で作成）、作成時/CI 時に*言語*へ、検出して修復できるか、それとも黙って通るかを問うた。

- **Almide はコンパイル時に 6/8 を検出**し、行動に移せる診断を出した。検出しなかった 2 件のうち、一つは純粋なロジックの off-by-one（対照例であり、型システムでは検出しない）、もう一つは追跡中の実際の Almide 診断不足（古いフィールド名を「unknown field」ではなく内部エラーと報告）を露呈した。
- **Python のコンパイルゲート（`py_compile`）は 0/8 を検出。** これらは構文コンパイルのゲートを通過し、その後、誤った値または実行時エラーを生じた。Python のすべての linter やテストの計測ではない。

`./run.sh` はそのうち三つを、実際のコンパイラ出力でその場で再現する（以下は一つ目）。

<a id="the-scenarios"></a>

## シナリオ

`./run.sh` は三つを順に示す。（1）網羅していない match（Python は黙って誤った**値**を出す）、（2）シグネチャ変更後の呼び出し箇所の更新漏れ、（3）値がない場合を処理せずに Option/None の結果を使うこと（Python はコンパイルゲートを通り、その後実行時にクラッシュする）。三つとも、Almide は行動に移せる診断を出し、誤ったプログラムのビルドを拒否する。#1 の詳細は以下のとおり。

AI に「`Shape` へ `Triangle` variant を追加して」と依頼する。AI は**型**を更新するが、`area()` で扱うことを忘れる。最もよくある変更ミスである。

自分で実行する：

```
./run.sh
```

<a id="1-correct-program--fully-type-checked-compiles-runs"></a>

### 1. 正しいプログラム — 完全に型検査され、コンパイルされ、実行される

```
$ almide run shape.almd
12
12
```

網羅的に型検査されている（手順 2 に注目。欠けた case は実行時の驚きではなくコンパイルエラーになる）。より深い**証明**の層は以下に説明する。

<a id="2-the-wrong-modification-in-almide--caught-at-compile-with-the-fix"></a>

### 2. **Almide** での誤った変更 — コンパイル時に修正方法付きで検出

```
$ almide run shape_buggy.almd
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape_buggy.almd:5:34
  in match
  here: fn area(s: Shape) -> Int = match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
  |
5 | fn area(s: Shape) -> Int = match s {
  |                                  ^
compile failed
```

失敗は**明確**（正確な欠落 case）で、**位置が分かり**（キャレット）、**修復可能**（追加する arm を渡す。Elm 級）である。これを読んだモデルは一段階で修正でき、人間も同様である。

<a id="3-the-same-wrong-modification-in-python--ships-silently"></a>

### 3. **Python** での同じ誤った変更 — 黙って出荷される

```
$ python3 shape_buggy.py
None
(exit 0)
```

コンパイルされ、実行され、0 で終了し、三角形の面積に `None` を返す。エラーはない。バグは本番へ届き、見つかるとしても下流のテストやエンドユーザーが見つける。

<a id="the-two-trust-layers-this-shows"></a>

## ここで示す二つの信頼の層

1. **失敗が明確かつ修復可能**（このデモ）：誤った変更を、行動に移せる診断とともにコンパイル時に検出する。モデルに依存せず、間違えるすべてのモデルに役立つ。
2. **モデル化した性質を検査する**（trust spine — 別の層）：`almide verify` は構造的な Wasm witness を生成し、独立した `almide-verify` に検査を委ねる。対応する core artifact は SHA-256 で bundle に結び付けられるが、この経路は component に対応しない。所有権の balance、名前、capability の境界には適用範囲と信頼の仮定がある。バイトの binding は実装済み（#4011）だが、完全なメモリ安全性や機能的正しさを確立するものではない。[gap 表](../../../../design/MEMORY-SAFETY.md) と [proven vs trusted](../../../../contracts/proven-vs-trusted.md) を参照。

実演する違いは、この例における行動に移せる診断と、黙って通る失敗または後で起こる失敗との違いである。証明書の根拠は別個の限定された主張であり、すべての誤った変更を検出すると確立するものではない。

<a id="honest-scope"></a>

## 率直な適用範囲

このデモは*失敗の明確さ*の軸（最も明快で、すぐ再現できる軸）を切り出す。ターゲット間比較と形式的 witness bundle は、適用範囲を明示した別の根拠の層である。`docs/` と `proofs/` を参照。ここで伝えるのは信頼の_体験_であり、モックではなく実際のコンパイラ出力で示す。
