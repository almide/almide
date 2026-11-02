<a id="cross-target-decisions-which-ones-read-one-source"></a>

# ターゲット間の判断：どの判断が単一の情報源を読むか

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cross-target-decisions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](cross-target-decisions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](cross-target-decisions.zh-CN.md)



> 原文：[docs/cross-target-decisions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/cross-target-decisions.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。読者向けの非規範的なスナップショット訳であり、自動同期されない。原文の時点・適用範囲・主張を維持したもので、翻訳時に独立した実測や実装検証は行っていない。 原文は 2026-09-29 の監査記録であり、現在の修正状況を示すものではない。

**判断**とは、コンパイラがプログラムについて答える意味論上の問いである。たとえば「この呼び出しは引数に書き込むか」「この式を後で実行してよいか」「これらのパイプライン段階を要素ごとに実行してよいか」といった問いだ。

両ターゲットは各問いに同じ答えを出さなければならない。native 側と wasm 側が**異なる正本**から答えを求めると、回答が食い違うことがある。その結果、同じプログラムが両側で異なる動きをする。通常、それぞれの側は内部で整合しているため、各側のテストでは気づかない。

この種の問題には、一方にだけ存在する表という共通の形がある。例は三つある。

- **lowering されるモジュールの集合。** native は解決済みの同梱標準ライブラリモジュールをすべて IR に lowering する。wasm 側（`src/wasm_leg.rs`）は、本体のない `= _` の公開面を持つ同梱モジュール、すなわち `list`、`map`、`string`、`bytes` などをすべてスキップする。
- **標準ライブラリ呼び出しの背後の実装。** native は Rust ランタイムの本体を呼ぶ。wasm はレジストリからセルフホスト実装をリンクする。
- 宣言にすでに書かれている内容を重複して持つ**手作業の一覧**。

このページは 2026-09-29 の監査時の一覧である。記録する規則は、一つの問いには**一つ**の回答関数を用意し、すべての側がそれを呼ぶというものだ。

<a id="the-inventory"></a>

## 一覧

| 問い | native の参照元 | wasm の参照元 | 現在の単一の参照元 | ゲート |
|---|---|---|---|---|
| 呼び出しはどの引数に書き込むか（branch lift は外側の変数に書き込む分岐の外部関数化を拒む） | lowering 済みモジュールの `IrFunction::mutated_params`。**修飾なし**の名前をキーにするため、モジュール関数が同名のユーザー関数を上書きしていた（#2948） | 同じ表だが同梱モジュールが含まれず（#2931）、後に宣言へのフォールバックを追加 | `almide_ir::mut_args`：標準ライブラリ関数はすべての側で宣言から、ユーザー関数は自身のスコープから回答 | `branch_lift::tests::every_stdlib_mut_fn_declines_the_lift_whether_or_not_its_module_is_lowered`。宣言から列挙し、lowering 済みコピーの表を意図的に誤らせる |
| レジストリからリンクされた標準ライブラリ呼び出しはどの引数に書き込むか（呼び出し地点の copy-on-write） | ランタイムの `&mut` 表（`generated/runtime_fn_modes.rs`）、次に宣言 | **リンクされた実装**の引数で、`mut` を持たない（#2949） | 公開面の宣言、`almide_ir::mut_args` | `declared_mut_agrees_with_runtime`（native の表 == 宣言、すべての `@intrinsic`） |
| キャプチャされた変数には共有ストレージが必要か（C-319） | `Mutability::Var`、またはクロージャ本体の `&mut` 借用 | 六つの bytes 書き込み関数と `Assign` の手作業リスト（#2951） | すべての標準ライブラリ呼び出しについて `almide_ir::mut_args` | `cells::tests::every_stdlib_mut_write_to_a_captured_var_makes_a_cell` |
| この `var` に書き込むことはあるか（`var`→`let` への降格） | 共通 lowering：`Assign` 系文のみ。このため `list.push(xs, ..)` だけで書き込む変数は `let` になり、native はクロージャへスナップショットとして取り込んだ（#2952） | 同じ。ただし wasm のセル走査はこの呼び出しを数えていた | 宣言された `mut` 引数に渡す変数は書き込みありと数える | 列挙による `use_count::mut_arg_write_tests` |
| この式を早く、遅く、二度、または一度も実行しないようにしてよいか | LICM：独自の `binop_may_trap` | 小さなスカラー関数のインライナ：「呼び出しなし」だけを条件にして、トラップする引数を落としていた（#2947） | `almide_ir::speculation` | 単体テスト、C-001 フィクスチャ |
| トップレベルの `let` の初期化式はいつ実行するか（C-007） | 遅延実行で、**直接の** `/`/`%` がある場合だけ起動時に強制（#2954） | 常に起動時 | 投機実行が安全でない場合に限り強制（`almide_ir::speculation`） | C-007 フィクスチャ |
| パイプラインの段階を要素ごとに実行してよいか（stream fusion） | IR 本体の純粋性に `EFFECT_MODULES`/`TOTAL_MODULES` と「中断する段階は最大一つ」を併用 | 独自の 7 モジュール拒否リストがあり、中断規則がないため、ユーザーモジュールのコールバック出力が交互に混ざった（#2953） | `almide_ir::fusion`：モジュールの分類語彙、トラップ規則、段階の実行順序規則 | 単体テスト、C-001 フィクスチャ |

<a id="checked-and-consistent"></a>

## 確認済みで整合しているもの

以下はそれぞれ実験または読解で確認し、両側が同じ参照元から答えるか、別の理由で整合していると分かった。

- **共通オプティマイザ：** fold、DCE、propagate、符号なし再 fold、optional-chain desugar。これらは構文的であり、側ごとの表を参照しない。DCE の `is_pure` はトラップする `BinOp` を純粋と扱う。どの側も同じなので、未使用の `let q = a / 0` はすべての側で落とされる。これは言語としての問いであり、ターゲット間の不一致ではない。
- **`mono` / `ir_link` / `top_let_storage::global_init_order`。** `program.modules` を読むが、その回答は同梱モジュールの存在には依存しない。
- **`mutual_tco` / `accum_tre`。** `IrParam::is_mut` に基づき `mut` 引数を持つ関数を拒む。これは両側で同じ IR である。
- **`fan`。** native は `fan { a; b }` ブロックをスレッドで実行し、wasm は逐次実行する。これは文書化された C-004 の interleaving の例外である。
- **LICM が停止しない呼び出しを反復回数ゼロのループより前へ移動すること。** 実験済みで、両側は一致する。
- **既存 MIR の線形化された `if`**（`almide-mir/src/lower/control.rs`、呼び出しのない分岐は両方実行）。トラップする分岐で実験済みで、両側は一致する。この既存実装は廃止作業中（#2930 / #2935）。

<a id="known-tracked-elsewhere"></a>

## 既知で、別の場所で追跡しているもの

- **#2503。** effect 呼び出し先は wasm の呼び出し地点の copy-on-write から除外され、レコードのフィールドである mut 引数も除外される（`param_mut` は `!is_effect` を要求し、`lower_mut_param_arg` は `Var` だけを受け取る）。native はどちらも束縛時に clone する。
- **`almide-mir/src/purity.rs`。** その `PURE_MODULES` は MIR lowering とインライナが読む capability-witness の受け入れリストである。これは三つ目のモジュール分類語彙で、`zlib` はここでは純粋だが fusion では effect である。現在はスカラーを返す標準ライブラリ呼び出しだけをインライン化本体へ受け入れ、それらは呼び出しのまま残るため、不一致は見つかっていない。`almide_ir::fusion` の語彙へ統一すべきである。

<a id="when-you-add-a-decision"></a>

## 判断を追加するとき

新しいパスが上記の問いのどれかを尋ねるなら、回答関数を呼び、答えを再導出しないこと。新しい問いなら、宣言、検査済みシグネチャ、IR ノードなど、**プログラムが表すもの**から答えること。各側がたまたま lowering またはリンクしたものから答えない。そのうえでここに行を加え、手作業の一覧ではなくレジストリを列挙するゲートを付ける。
