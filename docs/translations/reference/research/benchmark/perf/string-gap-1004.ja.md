<a id="where-the-string-workload-gap-goes-1004"></a>

# 文字列ワークロードの差はどこにあるのか（#1004）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](string-gap-1004.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](string-gap-1004.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](string-gap-1004.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)の非規範な履歴参照用スナップショット。固定コミット：35297a612fb29e3f78f6587aba253aa91d51f466。自動同期されない。測定、原因の寄与、実装説明、提案作業をここで独立に再検証・再実行していない。正本を優先する。日付とワークロードに限定された主張は、現在または普遍的な性能保証ではない。引用コマンドや提案変更は実行していない。

**答えを 1 行で：** 差の 4 分の 3 は `string.split` の戻り値型（`Vec<String>` — Almide には借用文字列型がない）、8 分の 1 は `string.len` の文字数という意味論による。疑われていたもの — RcCow 表現、rlib crate 境界、中間リスト — は全部合わせても 5 分の 1 未満である。同じ 2 つの要件を守る Rust 参照実装と比べると、コンパイラの出力は **1.04–1.12x** の範囲内にある。

2026-08-13、M4 Pro（14 コア）、`--release` で測定。以下のすべての数値はこのディレクトリ内の差分実験から得たもので、推測したものはない。

---

<a id="the-workload-and-the-two-references"></a>

## ワークロードと 2 つの参照実装

`strchurn/strchurn.almd` は #1004 のワークロードそのままである：N 個の整数 → `int.to_string` → `list.join` → `string.split` → `string.len` → `list.sum`。

Rust 参照実装は **2 つ**あり、それらを区別することで寄与を特定できる：

| 参照実装 | 処理内容 | 役割 |
|---|---|---|
| `rust-ref/strchurn.rs` | `split` が所有する `String` を収集し、長さは `chars().count()` | 同じ形／同じ意味論 — **意味のある比較** |
| `rust-ref/strchurn_idiomatic.rs` | `split` が借用した `&str` を生成し、長さは `s.len()` | 人が書くコード — API 契約の代価 |

この行は `check-perf-ratio.sh` が報告するが、固定基準にはしない。commit 95b9b6d59 が `listbuild` 群について定めた方針に従う。費用が割当てに由来するワークロードは、codegen より先に allocator を比較しており、almide/rust 比でもマシン差は相殺されない（同じ commit の listbuild が M4 Pro では 1.58、ubuntu-latest runner では 0.91）。listbuild と違い、ここでは両側が同じように割り当てるため、この行は固定基準にできるかもしれない。その前に第 2 のアーキテクチャの測定値が必要である。

2 つの要件はどちらも実在し、benchmark 用に作ったハンディキャップではない。Almide の `List[String]` の要素型は所有文字列なので、`almide_rt_string_split` は `s.split(sep).map(|x| x.to_string()).collect()` である。また `string.len` は文字数と定義されるため、`almide_rt_string_len` は `s.chars().count()` である。ワークロードは純粋な ASCII なので、3 プログラムすべてがバイト同一の出力を表示し、`bench.py` は時間測定前にそれを確認する。

<a id="headline"></a>

## 代表値

N=4M、交互に行う 21 回の実行の最小値を取り、そのような 3 バッチの中央値：

| バイナリ | ms | 出荷版との比 |
|---|---:|---:|
| `almide build --release`（出荷する rlib 経路） | 228.0 | 1.00 |
| 同じプログラム、monolithic build（`ALMIDE_NO_RTLIB=1`） | 218.0 | 0.96 |
| 同じ形の Rust 参照実装 | 205.3 | 0.90 |
| 慣用的な Rust 参照実装 | 111.5 | 0.49 |

**人が書く Rust に対して 2.04x。同じ仕事をする Rust に対して 1.11x。** issue の当初の 1.7x は、その N=1M では正確に再現する（89.7ms 対 53.8ms = 1.67x）。issue の「手書き Rust」は慣用的な方だった。

<a id="the-gap-attributed"></a>

## 差の内訳

方法は**段階的な比較**であり、`strchurn/ladder.py` として収録する。コンパイラ自身の `--target rust` 出力を取り、各段階で `__almide_main` の費用を 1 つだけ置き換え、各 runtime 関数はバイト同一のまま保つ。その後、参照実装と正確に同じ rustc flag で全段階をコンパイルする。両端で比較が閉じている。`v3_fused_sum` 段階は同じ形の参照実装へ到達し（211.7 対 205.3ms）、最後の段階は慣用的な参照実装へ到達する（111.5 対 111.5ms）。したがって差分は一部についての説明ではなく、差全体を説明する。

| # | 除去した費用 | ms | 比率 |
|---|---|---:|---:|
| 1 | `string.split` が N 個の**所有 `String`** を返す（各断片で malloc + memcpy + free、結果コンテナも 3x 太い） | 69.4 | **59.6%** |
| 2 | split 結果の**コンテナ自体**（サイズ hint のない `collect()` → 倍増による成長） | 17.6 | **15.1%** |
| 3 | `string.len` = `chars().count()` — O(n) の UTF-8 走査、ASCII 高速経路なし | 13.2 | **11.3%** |
| 4 | **rlib crate 境界**（runtime をユーザーコードへ inline 化できない） | 11.0 | **9.4%** |
| 5 | リストパイプラインの 3 つの中間要素を**合算** — 要素ごとの `Rc<dyn Fn>`、`list.range` の `Vec<i64>`、融合されない `list.map`→`list.sum` の `Vec<i64>` | 5.3 | 4.6% |
| 6 | 残る codegen（最後の段階と慣用的参照実装の比較） | 0.0 | 0% |
| | **合計** | **116.5** | **100%** |

1+2 行は 1 つの根本原因、`string.split` の戻り値型であり、合わせて差の **74.7%** を占める。

5 行目は意図的に 1 分類として報告する。1–4 行はバッチ間で数%以内に再現するが、5 行目内部の 3 段階はそうならない。4 バッチを通じた個別差分は 6.4 / −2.7 / 1.6 ms、次には 2.6 / 11.2 / −8.3 ms と出る。各々が共有マシンの ±3–8 ms のノイズフロア以下または同程度なのに対し、合計は約 5 ms で安定する。この N で 5 ms の分類を 3 分割しても、それは測定ではなく算術になる。その費用が測定可能な場所は以下の発見 (a) であり、そこでは 6.6x の価値がある。

<a id="the-counter-hypothesis-the-issue-names-refuted"></a>

## issue が挙げる対立仮説：反証

> 「`ALMIDE_NO_RTLIB=1`（monolithic LTO build）でも差は埋まらず、同じ 0.05s。したがって runtime の値表現が原因だ。」

前半は**確認され、さらに精密化された**が、後半は**誤り**である。

- crate 境界を閉じると出荷バイナリは 228.0 → 218.0 ms へ動くが、慣用的参照実装は 111.5 のまま。したがって境界は**実行時間の 4.4%／差の 9.4%** に相当する。実在する費用（`cargo_build.rs` コメントの「2–5%、文字列／リスト中心のループでは最大約 10%」という見積りは正しい）だが、差全体の説明には遠い。issue の当初のテストでは timer 分解能 10 ms で 4% が見えず、「効果なし」と報告した。それから引き出した結論は両方向に強すぎた。
- issue タイトルの表現は**全く関与しない**。`RcCow<T>` は `Bytes` と `Matrix` のみに使う（`codegen/templates/rust.toml` の `[type_bytes]` / `[type_matrix]`）。Almide の `String` は Rust の `String` へ、word 単位でそのまま lowering される。この経路に参照カウントの処理はない。出力された `__almide_main` を `RcCow` で grep しても現れない。**issue タイトルの前提は誤り。** 費用は文字列の表現方法ではなく、stdlib の signature が何個の文字列を存在させるかによる。

<a id="two-micro-optimizations-both-measured-both-rejected"></a>

## 2 つの小規模最適化：両方測定し、両方不採用

再導出する人が出ないよう記録する：

- **`almide_rt_string_split` で容量を予約する。** 倍増成長（2 行目）をなくすにはカウント走査が 1 回必要。4M 断片での測定：現行の `collect()` は **99.4 ms**、`Vec::with_capacity(matches.count()+1)` + `extend` は **115.1 ms**、カウント走査だけで **25.6 ms**。除去する成長費用より走査の方が高い。**差引き損失 — 導入しないこと。**
- **`almide_rt_string_len` の ASCII 高速経路**（`if s.is_ascii() { s.len() } else { s.chars().count() }`、一般にも厳密）。4M の短い文字列で `chars().count()` **10.7 ms** → 高速経路 **8.4 ms**。2.3 ms の改善 = **差の 2%** と引き換えに hot primitive に第 2 の走査形態を加える。丸め誤差程度の項目として作業一覧で保留。

<a id="two-findings-outside-the-string-row"></a>

## 文字列行以外の 2 つの発見

**(a) Rust ターゲットでは `IterChain` が一度も発火しない。** パイプラインの融合 iterator node は使われていない。`list.map` / `filter` / `fold` は pipe 内でも直接呼出しでも `list.range` 入力でも、実体化した `Vec` 上の `almide_rt_list_*(Vec, Rc<dyn
Fn>)` を出力する。このテスト群のどの benchmark の出力 `__almide_main` にも `.into_iter().map(…)` は現れない。動作するのは egg pass の combinator fusion で、`range |> map |> fold` は map を reducer に合成した単一の `almide_rt_list_fold` になる。したがって #1004 の背景が評価する融合は combinator レベルでは実在するが、iterator レベルにはない。`crates/almide-codegen/src/pass_stdlib_lowering.rs:426` の `try_lower_to_iter_chain` を「単一呼出し形状の fallback」とするコメントは古い。マッチ対象の `CallTarget::Module` を `ResolveCallsPass` がすでに書き換えているため、発火しない。

割当てが費用を隠さない軽い lambda 本体ではどの程度か（30M 要素、出力形状のモデル、5 回の最小値）：

| 形状 | ms |
|---|---:|
| `map` + `sum`、`Rc<dyn Fn>`、実体化した `Vec` 2 個（出力形態） | 36.1 |
| 同じもの、単相的 closure | 37.9 |
| egg fusion 後の `range` → `map` → `fold` — 実体化した `Vec` 1 個 | 31.5 |
| 完全に融合した Rust iterator chain | **4.8** |

**6.6x であり、boxed closure は理由ではない**。ここではその寄与は約 0。費用のすべては、`list.range` が `impl Iterator` ではなく、書いてから読み戻す実際の 240 MB の `Vec` であることによる。

**(b) `listbuild` 行の約 1.6x は #1004 ではない。** `perf/README.md` と `scripts/perf-ratio-baseline.txt` はともに、3 つの listbuild 行に残る実体化費用を「#1004 が追跡する実体化費用」と述べていた。違うし、実体化でもない。出力 Rust 内の `almide_rt_libm_sin`/`_cos` — クロスターゲットのバイト同一性契約が要求する決定的な **software libm** — だけを platform 版へ置換すると：

| バイナリ（2^23） | ms |
|---|---:|
| 出力された出荷形態 | 194.5 |
| 出力されたもの、platform の `sin`/`cos` | **105.5** |
| 手書き Rust 参照実装（platform の `sin`/`cos` 使用） | 123.9 |

この行の 1.57x 全体は超越関数の決定性による。platform libm を使うとコンパイラ出力は手書き参照実装を 1.17x **上回る**。疑われたものは無実だった。同形状の A/B では、`list.repeat` のゼロ埋め + 境界検査付き添字書込みが **75.5 ms**、`Vec::with_capacity` + push は **90.6 ms** と、出力形状の方が速い。`ALMIDE_NO_RTLIB=1` がこの行を動かすのは 0.6 ms。両コメントはこの変更で修正する。

<a id="verdict-and-work-list"></a>

## 結論と作業一覧

**文字列の数値を受け入れ、内訳とともに公開する。** 同じ仕事の Rust に対して 1.11x は恥ずべきものではない。`&str` の Rust に対して 1.9x は所有文字列 stdlib の代価であり、それ自体の価値を議論すべき言語設計上の決定であって、黙って修正すべき codegen 不具合ではない。

分類ごとの次の行動を、大きい順に挙げる：

| 分類 | 比率 | 次の行動 |
|---|---:|---|
| `split` → 所有 `String`（1+2 行） | 75% | これを動かすのは借用／slice 文字列型、または `string.split_iter` 型の consumer と融合する形態だけ。言語レベルの話であり、独自 issue と MSR の議論が必要（第 2 の文字列型は書きやすさの費用になる）。それまでは文書化され受け入れられた契約。 |
| `string.len` = `chars().count()` | 11% | 意味論は維持。profile が重要性を示したら、任意で 2% の ASCII 高速経路（上で測定）。 |
| rlib crate 境界 | 9% | hot な非 generic runtime 関数（`string_len`、`int_to_string`、`string_split`）への `#[inline]` が `cargo_build.rs` にある既定の計画。`check-embedded-size.sh` に対して導入する必要があるため、まだ導入していない。限定的だが実在し、実行時間の約 4%。 |
| 要素ごとの `Rc<dyn Fn>` | 5.5% | 単独の pass を設けるほどではない。軽い本体では約 0 と測定。下の項目が導入されれば付随して無料で消える。 |
| リスト中間要素（6+7 行、および発見 (a)） | ここでは約 1.5%、**別の場所で 6.6x** | **この文書で最大の実際の改善であり、文字列行の話ではない。** `IterChain` を復活させる（または `list.range` + combinator を遅延 Rust iterator へ lowering）ことで combinator chain の実体化をやめる。独自 issue が必要。`pass_stdlib_lowering.rs:426` の古いコメントも同時になくすべき。 |
| listbuild の 1.6x（発見 (b)） | — | #1004 の分類ではない。software libm の費用が高すぎると判断されたら、操作するのはクロスターゲット契約で保護した native-only 高速経路。codegen ではなく契約の議論。 |

<a id="reproducing"></a>

## 再現

```bash
cargo build --release

# the row, on the harness (native vs both references)
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/bench.py --legs native,rust --bench strchurn --runs 7

# the attribution table above, rebuilt from scratch
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/strchurn/ladder.py --runs 21

# the ratchet (prints strchurn's ratio in the reported, un-anchored group)
bash scripts/check-perf-ratio.sh
```

コードコメントの訳：harness 上で native と両参照実装の行を測定する。上の寄与表をゼロから再構築する。ratchet は strchurn 比を、報告のみで固定しない群に表示する。

`ladder.py` はプログラムを出力し、各段階で費用を 1 つ除く変更を加え、全段階がなお同じ出力を表示することを確認し、交互に時間測定して差分と比率を表示する。3 回実行すること。表の 1–4 行は再現するが、5 行目の内部の分割は再現しない（表の下の注記を参照）。また両端を閉じる 2 段階が参照実装からどれほど離れるかも表示する。それがゼロに近くなければ、差分はもはや差を説明せず、その表は測定ではなくなっている。

rlib 境界行を再現するには、同じプログラムを両方式で別々の新しい project directory にビルドする（build cache のキーは生成した Rust であり `ALMIDE_NO_RTLIB` ではないため、1 つのディレクトリを使い回すと他方のビルドのバイナリが黙って返る）：

```bash
ALMIDE_RUN_PROJECT_DIR=$PWD/.a  almide build .../strchurn.almd --release -o /tmp/rlib
ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR=$PWD/.b \
  almide build .../strchurn.almd --release -o /tmp/mono
```

---

原文に関する注意：結論は 1.9x とする一方、N=4M の代表値は 2.04x、N=1M の例は 1.67x であり、原文はこれらを整合させていない。作業一覧の比率（5.5% と約 1.5%）および「6+7 行」という参照も寄与表と一致しない。寄与表では中間要素を 5 行目に合算し、6 行目は残差ゼロ、7 行目はない。歴史的な不整合を修正したり独立測定とみなしたりせず保持した。測定した容量予約変更の不採用、ASCII 案の保留、iterator／文字列型／inline 変更の提案は異なる状態であり、提案を実装済みとはしていない。runtime 表現や古いコメントの主張は固定原文の時代のもので、新たなコード監査ではない。
