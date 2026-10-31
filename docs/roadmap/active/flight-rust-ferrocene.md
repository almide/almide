<!-- description: Native renderer and proposed Ferrocene qualification path; abstract RC lemmas do not prove Almide-to-Rust translation -->
# Flight Keystone (い) — v1 MIR → Rust → Ferrocene(忠実性束縛)

> **Goal**: [flight-profile](flight-profile.md) のラダー gate **G-F3** の具体実装設計 ──
> **Rust → Ferrocene を検討する設計案**。現行の native renderer は
> `crates/almide-mir/src/render_native.rs` に存在するが、その存在と
> Almide→Rust の全翻訳忠実性の証明は別である。
> `.github/workflows/ferrocene-lane.yml` は、Ferrocene 契約前の
> stock rustc を使用すると明記しており、資格化済みコンパイラでの実行実績ではない。
> 後段の資格化は前段の意味保存を自動的に証明せず、証明工数を 75% 再利用等の
> 割合から断定しない。
> **関連**: [flight-profile](flight-profile.md) §4 / [v1-mir-architecture](v1-mir-architecture.md) §1・§9 / [certificate-format-v1](certificate-format-v1.md)。

---

## 1. 設計核(一行)

> **設計目標**: native renderer の出力と明示した意味論を結ぶ根拠を整える。
> RC 事象列の既存定理は再利用候補だが、文字列パターン表と
> `exact eager_copy_refines_safety` だけでは実際の Rust 実行の忠実性は証明できない。
> 対象構文・ランタイム・ABI と未証明境界を明記し、後段ツールチェーンの証拠と分ける。

---

## 2. 既存証拠の分割（再利用可能性と未証明の接続）

| 区分 | ファイル | 飛行への含意 |
|---|---|---|
| **ターゲット非依存(そのまま再利用)** | `OwnershipChecker.v`(`check_sound` は**fold の `Z` 結果**を論証、`:34-36`)/ 証明書 i/a/d/m/r/b(= MIR 所有権事実)/ `certificate.rs`(target 認識ゼロ)/ `RuntimeModel.v` / `ALS.v` / `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` | 抽象事象モデルの定理は再利用候補。Rust 実行との接続は別義務 |
| **wasm 固有(唯一の target レイヤ)** | `Translation.v:28-35`(`wasm_pattern: Op→string`)+ `WasmEncode`/`WasmExec`/`WasmRcDec` + V validator | `rust_pattern` は構文照合の候補。意味保存の証明の厚さは未評価 |

`Translation.v` の eager instance は**既に証明済み**(`eager_translation_refines_safety
:= exact eager_copy_refines_safety`、`Translation.v:52-54` ← `ALS.v:62`)。

---

## 3. 当初の renderer 実装案(§A)

以下の demo/行番号は原設計の記録。現在の renderer が未着手という意味ではない。
実装の入口は `crates/almide-mir/src/render_native.rs` と `src/cli/emit.rs`。

**構造(`render_wasm.rs` を鏡映)**: `render_rust_program(prog) -> String` が
`render_rust_fn` を駆動 + `rust_preamble()`(`render_wasm.rs:648` `preamble()` のアナログ)+
self-host runtime リンク(`render_wasm.rs:591-644` `self_host_runtime()` と同形)。CLI 入口は
`examples/render_program.rs`(今 `render_wasm_program` を直書き :16/126)の兄弟。

**demo `render_rust.rs` から再利用(残す)**:
- op→Rust 対応スケルトン + §3.2 契約コメント(`render_rust.rs:1-21`)= **意図された写像、
  設計の規範アンカー**。
- `render_op` dispatch(`:120-198`)・`IntBinOp` lowering(`:172-189`、既にイディオマティック
  `a + b` / `(a < b) as i64`)・`CallFn`/`Call` 分岐(`:190-225`)・`render_rust_fn`(`:32-56`、
  tail-return-by-move 込み)。
- `value_reprs` 推論(`:85-112`)── heap-result 拡張(`CallFn.result` repr を読む、
  `render_wasm.rs:339-341` が既に持つ)を要する。

**本番で建て直す(review-grade の障壁)**:
1. **`rust_ty`(`:75-81`)が全 heap に `Vec<i64>`** ── 最大の障壁。`Repr::Ptr{layout}` /
   レイアウトレジストリを見て実型(`String`/`Vec<T>`/`Box<Node>`/named struct)を出す。
   DO-178C「ソース可読」は `Vec<i64>` 一律で落ちる。
2. **`var`(`:114-116`)が `v{id}`** ── `v0`/`v1` 番号は review 不可。`ValueId(u32)`
   (`lib.rs:114`)は名前を持たない ── **lowering 層で `debug_name: Option<Sym>` を side-table
   で通す**(IR `VarId` が source 名を持つ)。これは**lowering 変更**、レンダラ局所では不可。
3. **制御フロー**:demo は `If/Loop` マーカーを `None`(`:154-155`)= 誤り。flat マーカー列
   から**イディオマティック `if c { } else { }` / `while c { }` を再構成**(`render_wasm.rs:212-260`
   の `if_stack`/`loop_stack` の状態機械を鏡映)。**最大の net-new コード**。
4. **heap 初期化子**:`Init::Str`→実 `String`、`DynStr`→所有バッファ、`OptSome`→`Some(x)`。
5. **struct/enum 宣言**:demo に皆無。`walker/declarations.rs:58-62`(`#[derive(Clone)] pub struct`)
   `:135`(`pub enum`)を target に、`LayoutId` レジストリから出す decl pass。

---

## 4. `rust_pattern` 表 — 各 Op → イディオマティック Rust(§B)

§3.2 契約(`render_rust.rs:6-13`)の4アンカー(`Dup→.clone()` / `Drop→scope-end` /
`Consume→move` / `MakeUnique→no-op`)を全 op 集合(`lib.rs:152-257`)へ拡張。`dst`/`src` は
**解決済み実名**(`v{id}` でなく)、型は `Repr`/`LayoutId` の実型:

| Op | イディオマティック Rust | 注 |
|---|---|---|
| `Alloc{Init::IntList}` | `let mut dst: Vec<T> = vec![e0,e1,…];` | 実要素型 |
| `Alloc{Init::Str(s)}` | `let dst: String = "s".to_string();` | `vec![bytes]` を置換 |
| `Alloc{Init::DynStr}` | `let mut dst = String::with_capacity(len);` | |
| `Alloc{Init::OptSome{p}}` | `let dst = Some(p);` | |
| `Alloc{Init::Opaque}` | `let dst = Name { … };` | layout から ctor |
| `Dup{dst,src}` | `let dst = src.clone();` | §3.2。**唯一の所有権判断 = eager COW** |
| `Drop{v}` | *(何も出さない)* RAII scope-end | §3.2。eager 安全の源(`Translation.v:32` `Dec⇒""` と同じ) |
| `Consume{v}` | *(何も出さない)* use 地点で move | §3.2 |
| `Borrow{v}` | `&v` | |
| `MakeUnique{v}` | *(no-op)* `Dup` の `.clone()` で既に unique | §3.2 |
| `ConstInt{dst,value}` | `let dst: i64 = value;` | |
| `IntBinOp{Add/Sub/Mul}` | `let dst = a + b;` | demo `:172` |
| `IntBinOp{Div/Mod}` | `almide_div!(a,b)` / `almide_mod!(a,b)` | **trap-parity**(下) |
| `IntBinOp{Lt/…/Ne}` | `let dst: i64 = (a < b) as i64;` | demo `:181` |
| `IfThen/Else/EndIf` | `let dst = if cond != 0 { then } else { els };` | net-new、wasm `:232` 鏡映 |
| `LoopStart/BreakUnless/End` | `while cond != 0 { … }` | net-new、wasm `:214` 鏡映 |
| `SetLocal{local,src}` | `local = src;` | |
| `CallFn{dst,name,args}` | `let dst = name(args);` | |
| `Call{RtFn::*}` | self-host Rust fn 呼出(§5) | |
| `Prim{kind,…}` | 信頼床 → Rust stdlib(§5) | net-new |
| `Pure{dst,uses}` | `let dst = a.len();` 等 | demo は `0` に punt |

**trap-parity(難所)**: `Div/Mod` は MIR/wasm で 0 除算 trap(`render_wasm.rs:457` `i64.div_s`)。
素の Rust `a/b` は panic = 観測差。oracle byte 一致のため `almide_div!`/`almide_mod!`
(`rust.toml:36-40`)経由 ── 既に total-checked `Error:\n` + exit-1 形で review-grade。

**可読性戦略**(v0 と区別つかない出力に):等値 `almide_eq!`(`rust.toml:63`)、struct/enum
`#[derive(Clone)]`(`declarations.rs:58`)、concat `format!`(`rust.toml:53`)、runtime 呼出
`almide_rt_*`(`rust.toml:28`)、実名 + 実型。**litmus: `cargo fmt` 済み出力が人間 review を
通り `cargo build` が警告ゼロ**(CLAUDE.md codegen ルール)。

---

## 5. runtime ── MIR→Rust で dogfood、stdlib にマップしない(§C)

**推奨: self-host runtime を同じ MIR→Rust 経路でレンダリング(dogfood)、prim 床のみ Rust
stdlib にマップ。**

根拠([v1-mir-architecture](v1-mir-architecture.md) §4・§4.1):runtime は**Almide で書かれ
同じ Core→MIR→target を通る**。list/string/print を Rust `std` にマップすると **v1 が消すため
に存在する dual-implementation drift(~136 ルーチンの二重保守)を再導入**してしまう。runtime は
既に Almide で self-host・auto-link 済み(`render_wasm.rs:591-644` が `stdlib/*.almd` を lower、
`print_str.almd` は prim 床上の dogfood `println`)。Rust レンダラは**同一レジストリ**を再利用
── 同じ `.almd`・同じ `lower_function`、`render_rust_fn` に差し替えるだけ。**prim 床のみ**
(`PrimKind`、`lib.rs:262-273`)が Rust プリミティブにマップ ── これは各 target の target 固有
信頼床(`§4.1`)。`print_str` 本体は dogfood Rust に下り、`prim.fd_write` が `io::stdout().write_all`
に着く。

**ただし prim 床自体は target ごとに分岐する**(§7 難所3):Rust に共有線形メモリが無いので
`prim.handle(s)`(線形メモリのアドレス前提)は成り立たない。**prim 床より上は全 dogfood、
prim 床自体は target ごと分岐**(設計上の信頼境界、ここだけ「dogfood せず」が許される)。

---

## 6. 忠実性証明(§D)

### 6.1 抽象 eager 事象モデルの再利用例（Rust 翻訳証明ではない）

`rust_pattern : Op -> string` を追加(`wasm_pattern` `:28-35` と構造同型):

```coq
Definition rust_pattern (o : Op) : string :=
  match o with
  | Inc | Alias => ".clone()"   (* 新規取得 / alias = clone *)
  | Dec | MoveOut | Reuse => "" (* eager: scope-end drop / move、明示 op 無し *)
  end.

Theorem eager_rust_translation_refines_safety :
  forall ops, increments_only ops -> no_double_free ops.
Proof. exact eager_copy_refines_safety. Qed.
```

`Dec/MoveOut/Reuse` が空文字なのは **wasm と同じ理由**(RAII scope-end は明示命令を出さない、
eager wasm も出さない)。release-empty 補題(`is_release o → rust_pattern o = ""`)もそのまま移る。

- **既存定理の範囲**: `eager_copy_refines_safety` は抽象事象列の性質を
  示す。上の結論には `rust_pattern` も Rust 実行の意味論も現れないため、
  Rust 出力の正しさは導かれない。
- **別途必要な根拠**: 実際の生成コード・ランタイム・ABI がそのモデルを実現する
  対応関係。文字列一致や `Print Assumptions` だけでこの接続を閉じたとはしない。
- **残る接続義務**: Rust 断片が抽象 op を実現することの根拠は必要。後段の
  Ferrocene 採用だけでは、前段の生成 Rust とモデルの対応を省略できない。
  特定の形式化ファイルを必須と決めるのではなく、主張範囲に応じた証拠を定める。

### 6.2 per-build V validator(`translation_validation.rs` の `rust_pattern` 兄弟)

`wasm_pattern`(`:49-98`)+ `validate_translation_perceus`(`:122-126`)を鏡映し
`rust_pattern(op)->Option<String>` + `validate_translation_rust(rust_src, mir)`。**出力 Rust
ソース**を op 毎に走査:

| Op | wasm V(`:49-97`) | Rust V |
|---|---|---|
| `Dup`/`Alias` | `call $rc_inc` | `.clone()` |
| `Drop` | `call $rc_dec` | *(eager: 無 = leak-count 検査)* / *(perceus: scope-end `}` or `drop(x)`)* |
| `Alloc{IntList}` | `call $list_new` | `vec!` / `Vec::` |
| `CallFn{name}` | `call $name` | `name(` |
| `IntBinOp{Add}` | `i64.add` | ` + ` |
| `Call{PrintInt}` | `call $print_int` | `print_int(` |
| `Consume` | None | None(move) |

leak-count 検査(`:122-126` `rc_decs >= drop_count`)はそのまま移る。非空虚テスト
(`:185` パターンを剥がすと V が落ちる)も直接 port。`validate_safety` に `validate_safety_rust` 兄弟。

---

## 7. 信頼基底の台帳変更 + §9 和解(§E)

**信頼基底に入る(新たに信頼・未証明)**:
- **Ferrocene / 資格化 `rustc`** が Rust ソース→機械語を所有(ISO 26262 ASIL D / IEC 61508)。
  **資格化されている**(我々が証明するのでない)= SCADE KCG アナロジー(`certification-grade.md:135`)。
- **Rust prim 床**(`PrimKind`→ Rust stdlib `io::Write`/ptr)= 小さい閉じた信頼面、wasm prim 床の Rust アナログ。

**未証明境界**: Almide→Rust の全翻訳忠実性は、抽象 RC 定理や
`rust_pattern` の構文照合からは確立しない。Native 経路では Wasm の byte
束縛とは異なる検証対象になるが、Rust の生成コードとの対応義務が残る。
Ferrocene の資格化を採用しても、その前段の義務が消えるわけではない。

**§9 当初の設計目標**（以下の「証明束縛されたレンダラ」は達成済みの事実ではない）:
MIR のモデルを基準に生成 Rust との対応を確立する、という意図を保存する。

**原設計の和解案**(MIR 主権を尊重):`v1-mir-architecture.md §9` が却下したのは「**Rust を唯一の真**
にする」(Rust の無い形式意味論の上に建てる)。本設計は**それをしない** ── Rust 意味論の上に
建てず、認証済み **MIR 事実**に束縛、Rust は依然ただのレンダラ、**MIR は唯一の真のまま**。
唯一の変更は Rust レンダラを「ほぼ自明に下る踏み台」(§1)から「**証明束縛された2人目の本番
ターゲット**」に格上げすること。§9 が却下した「Rust = 真」と本設計の「Rust = 証明束縛
レンダラ」は別物。主権(wasm canonical)は保たれ、Rust は同じ witness の2人目の消費者。

---

## 8. Ferrocene 実証経路 + 難所(§F)

### De-risking 順(最小の端から端スライス先)

**Slice 0 ── `add` プログラム(両 target で既存)**。`render_rust.rs:245-286` `add_program`
(`fn add(a,b)=a+b` + `main` が 5 を print)は既に `rustc` でコンパイル・`5` を print
(`:278-286`)。要 op:`IntBinOp{Add}`/`CallFn`/`Call{PrintInt}`/`ConstInt`。**まず `rustc` →
Ferrocene `rustc` に差し替え byte 一致確認**。新レンダラコード無し・純 toolchain 配線。

**Slice 1 ── 値意味論リスト(所有権キーストーン)**。`render_rust.rs:313-342`
(`var a=[1,2,3]; var b=a; a[0]=9; print a,b`)= `Alloc{IntList}`/`Dup→.clone()`/
`MakeUnique→no-op`/`Drop`/`Call{ListSet,PrintList}`。既に `a=9,2,3 / b=1,2,3` を出す。
**§3.2 契約を実証**(1 `Dup` → 1 `.clone()` → 値意味論が by construction で正しい)。V 検査を追加。

**Slice 2 ── heap 型 + 制御フロー**。`Vec<i64>` を実 `String`/`Vec<T>` に(`rust_ty` 建て直し)、
実名を通し(lowering side-table)、`if`/`while` マーカー再構成を追加。最初のプログラム =
リスト総和の counted loop(`Loop*`/`SetLocal`/`IntBinOp`)= **キーストーン(あ)の counted-loop
サブセットと交差**([flight-wcet-loops](flight-wcet-loops.md))、2キーストーンが fixture 共有。

**最初の信頼できる Ferrocene デモに要るサブセット**: `Alloc{IntList,Str}`/`Dup`/`Drop`/
`Consume`/`MakeUnique`/`ConstInt`/`IntBinOp`(全)/`CallFn`/`Call{PrintInt,PrintList,PrintStr}`/
`SetLocal`/`If*`/`Loop*` + `print_str` の prim 床 = まさに [flight-profile](flight-profile.md)
§7 の飛行サブセット。G-F4 はその上で実安全臨界モジュール(制御則/状態機械/watchdog)を端まで。

### 正直な難所

1. **disjoint-paths(最深)**: Rust 経路が2本 ── demo `render_rust.rs`(op↔Rust 対応あるが
   `Vec<i64>`・非 review-grade・非 CLI)と `walker/`+`rust.toml`(review-grade だが**op 対応
   オブジェクト無し**・非 v1-MIR 駆動)。G-F3 は walker をそのまま採れない(検証する op↔fragment
   束縛が無い)し demo を出荷できない(非 review-grade)。**第3の物を建てる**: walker グレード
   Rust を出しつつ demo の per-op 対応を保つ本番 v1-MIR→Rust レンダラ。`flight-profile.md:144`
   が正直に sizing「render_wasm.rs の全言語版・約 5x」。**真のコスト = エンジニアリング、証明でない。**
2. **変数命名 = lowering 変更**: `ValueId(u32)` は名前を持たない。review-grade は実名要 ──
   IR→MIR lowering が `debug_name: Option<Sym>` side-table を通す。MIR データモデルに触るので
   両レンダラ + verifier に波及、wasm 側が無視できる `Option` にする。
3. **runtime レンダリングが prim-床-on-Rust ギャップを露出**: `print_str.almd` は
   `prim.handle/load32/store32/fd_write`(線形メモリへの生アドレス)で書かれる。Rust に共有
   線形メモリモデルが無い。prim 床は (a) byte-addressed arena をエミュレート(忠実だが醜い・
   可読性を損なう)か (b) `RtFn::PrintStr` 境界で `print_str` を Rust ネイティブ `String` write
   にマップ。**推奨: prim 床より上は全 dogfood、prim 床自体は target ごと分岐**(設計上の信頼
   境界でのみ「dogfood せず」が曲がる)。
4. **Repr/ABI 決定性**: 実 `String`/`Vec<T>`/struct はレイアウトを rustc に委譲 ── **観測**
   等価(stdout バイト)には十分だが**byte-layout** 同一ではない。観測等価が契約(CLAUDE.md
   Behavior Contracts)なので許容、`almide_div!`/`almide_mod!` が唯一の trap 差を処理。FFI/ABI
   保証を後で主張するなら `--repr-c`(`rust.toml:56`)が効く。

**正味**: 既存 native renderer と抽象モデルの証拠を土台にできるが、
全翻訳忠実性・実際の資格化ツールチェーン使用・当局受理は別々の未確立事項である。
元の ~5x/~75% 見積りや「表一つで自明」という説明を、現在の工数・証明完了の
根拠として使わない。
