<a id="reviving-the-monkey-regression-suite-what-spec-actually-is"></a>

# monkey 回帰テスト群の復活：`spec/` の実態

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](mir-caps-call-count-breach.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](mir-caps-call-count-breach.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](mir-caps-call-count-breach.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mir-caps-call-count-breach.md)の非規範な履歴参照用スナップショット。固定コミット：35297a612fb29e3f78f6587aba253aa91d51f466。自動同期されない。原文の主張、件数、診断、現時点の再現状況はここで独立に検証しておらず、例、コンパイラのコマンド、ゲートも実行していない。正本を優先する。歴史的提案は、復元、コンパイラ変更、ゲート方針変更の承認ではない。

原文の説明（表示のみ）：2026 年 8 月の monkey テスト群の caps 計上診断、復元された公開来歴、未完了の現時点での再現。

<a id="reading-status-2026-10-31"></a>

## 読む際の位置付け（2026-10-31）

これは **2026-08-08 の診断**を、15 件の wall の集計、2 件の MIR>IR 報告、却下された修正案を含めて保存したものである。新たに再現した件数でも、現在の CI が失敗しているという主張でもない。元のコーパスは [公開履歴の `48621335`](https://github.com/almide/almide/tree/48621335e54af937dd4bb8432ba14c525c310842/spec/regression) にあり、その後の [revert の理由](https://github.com/almide/almide/commit/a5ebd06937b533dd2609a5bdc4a7daa961b4c6a7) も公開されている。当時の入力を復元するために非公開アーカイブは必要ない。

現時点での再現は未完了である。最小例は frontend の `almide check` を通過するが、このコマンドは `classify_corpus` の MIR/IR 呼出し数の安全網を実行しない。不具合を修正済み、または今も再現可能と呼ぶには、固定した classifier のビルドと報告が必要である。文書を完成したように見せるためだけに、テスト群を復元したり、縮小のみ許される baseline を拡大したりしてはならない。以下の設計案は歴史的なものであり、新たに承認されたコンパイラやゲート方針の決定ではない。

<a id="original-report"></a>

## 当初の報告

**発見時の状態**：未解決。2026-08-08、リポジトリ直下の `_scenarios/` に置かれ、CI が `almide test spec/` しか実行しないため未実行のまま放置されていた monkey 回帰テスト群（`MONKEY_BUGS.md`、41 ファイル、2026-04-05 に v0.11.4 で作成）を復活させる際に発見した。

テスト群は正しく、通過する。41 ファイルすべてが両ターゲットで成功する（これらを含む `almide test` は 393 ファイル、失敗 0）。失敗を引き起こすのは `spec/` への移動であり、その理由がこの文書の要点である。

<a id="spec-is-the-v0-corpus-not-a-test-directory"></a>

## `spec/` はテストディレクトリではなく v0 コーパス

`proofs/corpus-wall.sh`（CI ジョブ `Trust Spine (v1)`、`make verify-trust`）は `$ROOT/spec` のディレクトリ全体を実際の frontend → MIR に通し、3 つの性質を強制する。そのうち 2 つは**縮小のみ許されるラチェット**である。したがって `spec/` へのファイル追加は無条件にはできない。v1 lowering がまだ扱えないソースの形を追加するとラチェットが増え、ゲートは方針を明示している：

```
WALLED-REAL RATCHET FAIL: function(s) walled that are NOT in proofs/walled-real-baseline.txt —
ship the lowering in the same change (adding entries is a reviewed regression, not a fix)
```

上の診断文の訳：WALLED-REAL RATCHET FAIL — proofs/walled-real-baseline.txt にない関数が wall で遮断された。同じ変更で lowering を提供すること（項目追加はレビューを要する後退であり、修正ではない）。

復活させた 41 ファイルは、7 ファイルにわたり **15 個の新しい walled-real 関数**を加える：

| ファイル | 新しい wall 数 |
|---|---|
| `scenario8_error_taxonomy_test.almd` | 3 |
| `scenario3_pipeline_test.almd` | 3 |
| `monkey24_variant_advanced_test.almd` | 3 |
| `monkey08_option_result_test.almd` | 2 |
| `monkey05_pattern_match_test.almd` | 2 |
| `monkey13_records_test.almd` | 1 |
| `monkey14_edge_cases_test.almd` | 1 |

いずれも通常の Almide である。match 分岐の guard、heap-result の `match`/`if` 末尾、追跡されていない Option オペランド上の `??`、クロージャのリスト引数を持つ `list.filter_map`、呼出しを含む分岐を持つ未追跡対象への `match`。それぞれ、現行の `spec/` コーパスが試していない実際の v1 lowering の欠落である。

<a id="plus-one-hard-breach-mir_calls--ir_calls"></a>

## さらに 1 種類の重大な違反：`mir_calls > ir_calls`

2 ファイルは、baseline がなく必ず 0 でなければならない **caps 健全性の安全網**にも違反する。`record_elided_calls` マーカーが表面化させてよいのは本当に省略された呼出しだけなので、MIR 呼出し数が増えても IR の数までである。それを超えるなら、マーカーが lowering 済みの呼出しを二重計上している。それは別の省略を隠し、Stdout に到達する関数の taint を誤って解除し得る。

```
MIR>IR monkey28_nested_data_test.almd::profile_summary (mir 8 > ir 7)
MIR>IR scenario5_plugin_test.almd::merge_sources        (mir 3 > ir 2)
```

<a id="minimal-repro"></a>

### 最小再現例

```almide
type DS = { fetch: fn(String) -> Result[List[String], String] }

effect fn merge(sources: List[DS]) -> List[String] = {
  var all: List[String] = []
  for src in sources {
    let rows = src.fetch("*")!
    all = all + rows
  }
  all
}

test "t" { assert(true) }
```

```
$ ALMIDE_DEBUG_CALL_OPS=1 cargo run -p almide-mir --example classify_corpus -- --out /tmp/o <dir>
[call-op] merge: CallIndirect              <- src.fetch("*")
[call-op] merge: CallFn __list_concat_rc   <- all + rows
[call-op] merge: CallFn __str_concat       <- SPURIOUS: the source has no string concat
[call-count] ir=2
MIR>IR <dir>/a_test.almd::merge (mir 3 > ir 2)
```

上の出力注記の訳：SPURIOUS は余分な呼出しを示す。ソースには文字列連結がない。

`ir=2` は正しい（`Call` が 1 つ、`ConcatList` が 1 つ。`count_ir_calls` はすべての `ConcatList` ノードを計上する）。3 つ目の MIR 操作が不具合である。ソースのどこにも `ConcatStr` ノードがない関数に `__str_concat` の `Op::CallFn` が存在する。

<a id="root-cause-confirmed-2026-08-08"></a>

### 根本原因（2026-08-08 確認）

`build_append_slot_subst`、`crates/almide-mir/src/lower/mod_p5_b.rs:513-527`。アキュムレーターループを末尾再帰の append-slot 形式へ lowering する際、アキュムレーターのコピーを合成する。関数自身のドキュメントコメントにもこうある：*「String アキュムレーターは `acc + ""` で、リストは `acc + []` でコピーする」*。

```rust
let (empty, concat_op) = if matches!(list_ty, Ty::String) {
    (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), BinOp::ConcatStr)
} else {
    (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), BinOp::ConcatList)
};
let copy = tco_ir(IrExprKind::BinOp { op: concat_op, left: Var(acc_var), right: empty }, ..);
```

この `acc + ""` は、**ソースには存在しなかった `ConcatStr` ノード**である。`try_lower_concat_str`（`lower/calls_p2.rs:140`）が実際の `Op::CallFn { name: "__str_concat" }` へ lowering するので、`mir_calls` に呼出しが 1 つ増える。しかしソース本体を走査する `count_ir_calls` には計上するノードがない。したがって `mir 3 > ir 2` となる。emitter に計測を入れて直接測定した結果：

```
[sc] __str_concat  ty=String
     left  = Var { id: VarId(7) }
     right = LitStr { value: "" }
```

調査途中で原因から除外したもの：`record_elided_calls`（`lower/calls_b.rs:170` — この関数ではその push は何も表示しない）と `lower_interp_compound_wall`（`lower/calls_p2.rs:592` — 再現例には文字列補間がない）。

<a id="the-fix-is-a-decision-not-a-patch"></a>

### 修正にはパッチではなく決定が必要

注入されたコピーは `$__mg_take` と同じ種類であり、ゲートは後者をすでに名前で除外している。その理由は次のとおり：

> コンパイラが注入したスロットアクセサーであり、IR 呼出しノードの lowering ではない。可変グローバルのヒープ代入は IR の対応物なしにこれを注入するため、数えると `mir <= ir` の誤った違反になる。

しかし `__str_concat` を単純にそのフィルターへ加えることはできない。本物のソースレベルの文字列 `+` も `__str_concat` へ lowering され、ゲートがソースの各 `ConcatStr` ノードを 1 ir_call と計上するのは、まさにそれらを数え続けるためである。名前を丸ごと除外すると実際の呼出しを過少計上し、安全網を弱める。その目的とは正反対である。影響の小さい順に、3 つの解決案がある：

1. **注入したコピーに別名を付ける**（`__str_copy` / `__acc_copy`）。`__mg_take` と同様に、ゲートがそれだけを正確に除外できる。リンクすべき self-host シンボルが増える。
2. **IR 側で計上する** — String の append-slot アキュムレーターは合成 concat を 1 つ意味することを `count_ir_calls` に教える。出力プログラムは変わらないが、ゲート内に lowering の振り分け規則を複製することになり、まさに `__mg_take` のコメントが警告する乖離を生む。
3. **`ConcatStr` なしでコピーする** — `x + ""` という恒等式の手法を再利用せず、append-slot lowering に専用のコピー操作を与える。

(3) が最も明快である。コピーは連結ではなく、`x + ""` が便利な恒等式だったために連結のように見えるだけだからだ。ただし所有権の証明書で保証する lowering を変更するため、ついでの修正ではなく独自のレビューが必要になる。どれを選んでも受入れテストは同じである。`spec/regression/` を復元し、`proofs/corpus-wall.sh` が違反 0 を維持することを確認する。

<a id="state-and-proposed-recovery-at-the-time-2026-08-08"></a>

## 当時の状態と復旧案（2026-08-08）

`spec/regression/` は revert された。41 ファイルとこの再現例は作業アーカイブ（`../almide-docs-archive/blocked-on-mir-caps-gate/`）および git 履歴（`git log --diff-filter=D -- spec/regression/`）にある。

テスト群を取り込むには、次の順に実作業が必要である：

1. **caps 計上の欠落を解消する**（2 件の重大な違反）。これは健全性の安全網であり、baseline に入れるのではなく修正しなければならない。
2. **15 件の lowering の欠落を解消するか、意識的に baseline へ入れる。** ラチェット自体の方針は「同じ変更で lowering を提供する」である。15 項目の baseline 化はレビューを要する後退であり、ついでの変更ではなく決定が必要になる。
3. `spec/regression/` を復元する — 復元が受入れテストである。

それまではテスト群を `spec/` の外に置く。入れると `Trust Spine (v1)` が失敗し、テストの価値がいかに高くても健全性ゲートの失敗を正当化しないからである。すでに得られた価値はこの文書である。`spec/` が 4 か月拡大しても表面化しなかった、未対応の lowering 形状 15 件と 1 種類の計上違反を明らかにした。

---

原文に関する注意：8 月の報告は不具合を未解決、テスト群の導入で trust ゲートが失敗すると述べるが、10 月の読解上の状態は現時点の再現を明示的に未完了としている。これは異なる歴史的な範囲の記述であり、現在の失敗や修正の証拠ではない。リストを使った最小例と String の合成コピーという診断は原文どおり保持し、classifier を再実行して関係を解明してはいない。「1 件の違反」は計上問題を指し、検査違反のファイル／報告は 2 件ある。公開履歴リンクと非公開アーカイブ参照を保持した。後の状態注記によればアーカイブは不要である。テスト群の復元や baseline 拡大は行っていない。
