<a id="almide-wasm-vm--requirements"></a>

# almide-wasm-vm — 要件

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REQUIREMENTS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](REQUIREMENTS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](REQUIREMENTS.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-vm/REQUIREMENTS.md)
> 資格認定に関する記述は原文の根拠と要件であり、資格認定や認証を取得済みという意味ではない。fuel は既定で無制限であり、有限 fuel による保証には deployment の指定が必要。記載の Critical capability 制限と 5 import の境界は原文要件として保持し、すべての host や artifact へ一般化しない。
> 原文の不一致を保持：この履歴要件文は IO の許可がファイルシステムへ到達すると述べる一方、固定版の capability-system 文書は IO と FS を区別する。この旧記述から現行の権限動作を推論せず、正本の能力仕様を確認すること。

Almide の wasm 出力に対する、資格認定の対象にできる実行環境（#865）。汎用ランタイムには現実的な資格認定への道筋がない。JIT であり、その機能範囲が Almide の出力を大幅に超えるためである。この crate は代わりに、Critical profile が言語を狭めるのと同じように、対象範囲を狭める。Almide の出荷成果物に含まれ得る命令とモジュール機能だけを解釈し、それ以外は何かを実行する前にすべて拒否する。

各要件は、それを支える証拠を挙げる。実行可能な証拠がない要件は主張しない。

| Id | 要件 | 証拠 |
|---|---|---|
| REQ-VM-1 | **入力は出荷成果物。** VM は `almide build --target wasm` が書き出すバイト（structural emitter のモジュールに `to_wasi` 変換を適用したもの）を変更せず実行する。 | `tests/wasm_vm_parity_test.rs` はすべての `spec/wasm_cross` fixture を製品 CLI で build し、書き出されたファイルを実行する。 |
| REQ-VM-2 | **閉じた集合。** 受理する命令は emitter と `to_wasi` shim が書き出せるものに厳密に一致し、122 個である。受理するモジュール機能も閉じている。型は最大 1,000 個の引数を取り、返す値は最大 1 個。宣言型は i32、i64、f64 のみで、f32 はオペランドスタック上に一時的に存在するだけ。ブロック型は 1 バイト形式のみ。funcref table は最大 1 個、エントリ数は最大 2^20。非共有の 32-bit memory が厳密に 1 個存在し、WASI command の要件どおり `memory` として export する。global の初期化子は定数。element と data segment は active のみで、table と memory 0 を対象にし、data count は data と一致する。export は存在する index を互いに異なる名前で指す。custom section はスキップする。start section はなく、`_start` は何も受け取らず何も返さない。それ以外は load 時に拒否し、1 バイトが決定要因である場合はそのバイトの offset を示す。 | `tests/allowlist.rs` は VM の集合と emitter のソースを双方向に比較する。`tests/refuse.rs` は、text assembler が書き出さない手作り encoding を含む拒否を扱う。 |
| REQ-VM-3 | **実行前に、上限のある load 内で検証。** 各本体は、閉じた集合に限定した標準検証アルゴリズムを 1 pass で通過する。同じ pass で平坦な形式へ変換し、その分岐は解決済みの target、operand height、保持する値を持つ。未検証の本体の命令は実行しない。load の時間とメモリはモジュールサイズに比例する。宣言 locals は run 単位で保持するため、大きな個数も 1 entry で済む。型は線形時間で正規化する。割り当てを行うすべてのサイズ、すなわち引数、本体ごとの locals、table entries、memory pages に上限がある。 | `tests/refuse.rs` は型不正の本体、偽装サイズ、ごみや途切れた入力を対象とし、panic せず拒否しなければならない。 |
| REQ-VM-4 | **ホストの範囲。** WASI preview-1 import 5 個だけを、それぞれ厳密な署名で link する。`fd_write` は stdout と stderr に対応する。`fd_read` は stdin に対応し、標準ランタイムと同じく 1 呼び出しにつき最初の空でない buffer を満たす。`proc_exit` は指定 code で実行を終了する。`random_get` と `clock_time_get` は自分の名前を示して trap する。これらは Rand と Time capability を支え、Critical program には決して付与されない。不正な file descriptor は EBADF を返す。memory の外の pointer、または 4-byte 値への alignment が不正な pointer は、標準ランタイムと同様、何かを書き込む前に trap する。 | `src/wasi.rs` の単体テストは、自身の iovec 配列への read も含む。`tests/run.rs` は `stdin_is_served_and_the_unserved_calls_trap_by_name` と `a_bad_pointer_handed_to_the_host_traps` を扱う。 |
| REQ-VM-5 | **固定メモリ。** すべての buffer は instance 作成時に割り当てる。対象は cap までの linear memory、value stack、frame stack。実行中は何も割り当てない。呼び出しは入る前に、callee の locals と検証済み最大 operand height を合計した stack 要求全体を確認する。どちらかの固定境界を超えると `call stack exhausted` で trap する。cap を超える `memory.grow` は -1 を返す。 | `tests/run.rs` は `recursion_past_the_fixed_depth_traps`、`memory_grows_to_its_cap_and_no_further`、および 4 frame で 100 万回の tail call を実行する `a_tail_call_reuses_its_frame` を扱う。 |
| REQ-VM-6 | **Fuel。** 平坦な形式の各命令は、実行時に fuel 1 単位を消費し、尽きると trap する。block、loop、end marker は命令に変換されず、消費しない。loop の back edge は branch で 1 を消費するため、fuel はすべての実行を制限する。host call や bulk memory 命令も長さにかかわらず 1 を消費し、その仕事量は buffer と memory size によって制限される。 | `tests/run.rs` は `fuel_bounds_every_run` を扱う。 |
| REQ-VM-7 | **Trap は仕様どおり。** 整数のゼロ除算と overflow、範囲外 memory、不正な indirect call はすべて WebAssembly 仕様どおり trap する。不正 call は null entry、table 外の index、署名不一致。trap 後は、それ以上命令を実行せず host call もしない。 | `tests/run.rs` は数値、memory、indirect call の各ケースを扱う。`src/numeric.rs` の単体テストは境界ケースを扱う。 |
| REQ-VM-8 | **Runner の契約。** return した実行は exit 0。`proc_exit` は native や embedded host と同様、渡された code のまま exit し、OS が下位 8 bit を保持する。VM は status を狭めない。C-350 はコンパイラ側に preview-1 artifact の `proc_exit` を 0..=125 に保たせ、この runner の仕事は渡されたものを実行すること。trap は exit 1 とし、embedded host と同じ表記の stderr 1 行 `Error: wasm trap: <reason>` を書く。program の最後の stderr 行がすでに `Error: ` で始まり、trap が `unreachable` の場合はその行を省略する。これは embedded host が従う die 規約である。拒否された module は exit 2。 | `tests/run.rs` は exit code と trap 行のテストを扱う。`src/wasi.rs` は `the_last_line_is_judged_as_str_lines_would` を扱う。 |
| REQ-VM-9 | **標準ランタイムおよび native との同値性。** 各出荷 artifact は 3 つの結果のいずれかになる。import がすべて 5 個の中にあれば load して実行し、標準ランタイムの stdout、stderr、exit code と一致する。trap は理由と program 自身の stderr で比較する。VM が 1 行を伴う exit 1 を返す一方、標準ランタイムは独自の status と error block で exit するためである。または、標準ランタイムの出力の prefix を出した後、clock や entropy 呼び出しで名前付き trap に停止する。それ以外の artifact は load 時に拒否する。出荷 artifact は、標準ランタイムが拒否する 126 以上の status で `proc_exit` を呼べない。C-350 は preview-1 build 自身に、`proc_exit` の前に独自 stderr 行と exit 1 で 126..=255 を拒否させる（#2303、#2780）。したがってそのような status はコンパイラ自身の約束違反であり、gate は相違と判定する。profile の deny-all default である `almide check --profile critical` に合格するすべての fixture は同じ実行結果となり、native binary の実行とも一致する。 | `tests/wasm_vm_parity_test.rs`。release-only であり、CI の commissioned wasm gates job で実行する。 |
| REQ-VM-10 | **信頼基盤。** Rust 標準ライブラリ以外に依存はなく、workspace lint を通して `unsafe_code` を禁止する。test-only crate、すなわち test module 用 text assembler は基盤に含まれない。 | `Cargo.toml` は空の `[dependencies]` と `[lints] workspace = true` を持つ。 |

<a id="scope-and-what-it-leaves-out"></a>

## 対象範囲と対象外

- **1 つの artifact 系統。** VM は structural emitter が生成する preview-1 artifact を実行する。component（`--component`）は別形式であり対象外。
- **Critical profile の既定の範囲。** console output、exit code、stdin を意味する。`--allow IO` の付与は file system にも到達するが、file system、environment、argument access はすべて 5 個以外の呼び出しを import するため、それらの artifact は load 時に拒否する。clock と entropy は名前を示して trap する。これらが無言の誤答になることはない。file system に対応すると、信頼する host の範囲を倍増させることになり、この crate の目的に逆行する。
- **制限は定数でなく引数。** 既定値は 32-bit memory ceiling、4 Mi-cell value stack、256 Ki frames、無制限 fuel。corpus の全 program が標準ランタイムと同様に実行できるよう設定している。deployment は `--fuel`、`--memory-pages`、`--stack-cells`、`--call-depth` で自身の境界を指定する。
- **将来の切り出し。** 実際の資格認定へ進む段階になれば、凍結した release lifecycle を持つ独自 repository へ移す。一緒に移す必要がある結合点は allowlist test である。この test が VM の集合を emitter の集合へ固定する。
