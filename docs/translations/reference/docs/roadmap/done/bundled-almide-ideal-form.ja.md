<a id="bundled-almide-stdlib--ideal-form"></a>

# 同梱 Almide Stdlib — 理想形

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bundled-almide-ideal-form.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bundled-almide-ideal-form.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bundled-almide-ideal-form.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-ideal-form.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去のメタデータ：説明 — 同梱 Almide stdlib の理想形：dispatch 経路は一つ、パッチ層の特例なし。完了日 — 2026-04-17。

> 原文の注意点：完了バナーは負債 5 項目の解消を示す一方、保存された当初の本文には「未解決項目」と残作業の見積りがある。これらは時点の異なる記述であり、現行課題を新たに未解決へ戻したものではない。MSR の根拠と時間見積りは、新たな測定や納期の約束にせず保持する。

> **状態**: v0.14.7-phase3.N の一連の作業で、負債 5 項目をすべて解消した（`codegen-ideal-form.md §Phase 3 Arc` の S1 → A）。完了時のパッチ層監査は `CHANGELOG.md §0.14.7-phase3.5` を参照。以下には当初の限定範囲の根拠を保存する。

`bundled-almide-dispatch` の作業（v0.14.6 で出荷）により `stdlib/<m>.almd` が使えるようになった。しかし最初の実装では同梱 fn をトップレベルのユーザー fn と別物として扱ったため、パッチ層の特例がいくつか増えた。理想形は同梱 fn を所属モジュールの第一級メンバーとして扱う。パイプラインのどこにも脇道を作らない。この文書は、0.14.7-phase3 にかけて解消するために、残った負債を一覧にする。

<a id="guiding-principle"></a>

## 指針

> 同梱 stdlib fn は、型チェッカー、単相化器、codegen dispatcher から見て、トップレベルのユーザー fn と区別できないものであるべきだ。fn に Rust/WASM のランタイム本体があるかを知るのは、**TOML ランタイムマッピング**だけである。

以下の残項目はいずれも、現在のコードが「同梱か否か」の特例を持ち、理想形で消すべき箇所である。

<a id="open-items"></a>

## 未解決項目

<a id="1-single-dispatch_module_call-entry-per-target"></a>

### 1. ターゲットごとに `dispatch_module_call` の入口を一つにする

現時点の codegen には、モジュール fn に到達する箇所が二つある：

- **Rust**: `pass_stdlib_lowering` は、fn に TOML エントリーがある場合に `Module { m, f }` → `Named { almide_rt_m_f }` と書き換え、なければそのまま残す（walker が残りをモジュール呼出しとして出力する）。
- **WASM**: `emit_wasm/calls.rs` に手書きの `match module` があり、`emit_list_call` / `emit_int_call` / その他へ分岐する。各モジュールの fallback は `func_map.get("almide_rt_m_f")` を試す。

理想形は、ターゲットごとに入口を一つにし、優先順位を次のとおりにする：
1. IR モジュールに `f` という fn がある → ユーザー fn の呼出し経路（同梱 fn も扱う）。
2. TOML registry に `(m, f)` がある → 既存の inline emit。
3. どちらもない → 実行時の `stub_call → unreachable` ではなく、**コンパイル時 ICE**。

根拠：現在は `list.catt(xs)` のようなタイプミスが黙ってコンパイルされ、最初の呼出しで trap する。理想形ではこれを診断に変える。

<a id="2-retire-emit_stub_call--unreachable"></a>

### 2. `emit_stub_call → unreachable` を廃止する

`emit_wasm/calls.rs` の `emit_stub_call_named` には、構造上必ず到達できる（WASM emit はすべてのモジュール dispatch に `_` arm を持つ）。この fallback があるのは、WASM emitter に統一された「未知の fn」の ICE 経路がなく、部分的に妥当な IR の出力中にクラッシュさせたくないためである。

(1) が入れば stub 経路はなくなる。WASM emit は成功するか、不足する fn を名指ししたコンパイル時エラーを返す。

<a id="3-concretizetypes-hard-postcondition"></a>

### 3. `ConcretizeTypes` の強制的な事後条件

`ConcretizeTypes` には `ALMIDE_AUDIT_TYPES=1` で有効になる緩い監査があり、pass 後に残る未解決型を指摘する（`pass_concretize_types.rs` の冒頭コメント参照）。現時点で、残存型の既知の発生源が二つある（一部の `list.zip` ケースの Call 戻り値型）。それらを解消して監査を強制的な事後条件にし（debug では panic、release では error）、環境変数を除去する。

同梱 Almide にとって重要な理由：同梱 fn は、WASM emitter が `i32` に fallback する位置まで TypeVar を持ち込むことがある。これは今回の作業で split_at の失敗を覆い隠したバグの種類である。強制的な事後条件は、その種類を IR 検証時に捕捉する。

<a id="4-unify-optionresult-almd-signatures-with-toml"></a>

### 4. option/result の `.almd` シグネチャを TOML と統一する

`stdlib/option.almd` / `stdlib/result.almd` は TOML のシグネチャを上書きする（`fn() -> X` 対 `Fn[Unit] -> X`）。すべての呼出し元が `() => x` と書くため、.almd ファイルを除去するとテストスイートが壊れる。三つの経路と推奨案（案 A：TOML を `Fn[] -> X` に正規化する）は `option-result-bundled-cleanup.md` を参照。

シグネチャの上書きが残る間は、型チェッカーが TOML シグネチャに対する特権を同梱 fn に与えているので、正直に「同梱 fn = 第一級メンバー」とは言えない。これが項目 (5) を妨げている。

<a id="5-first-class-mono-for-module-fns--done-verify-coverage"></a>

### 5. モジュール fn の第一級単相化 — 完了、カバレッジを確認する

v0.14.6 で追加した `monomorphize_module_fns` は、Rust と WASM の両方でジェネリックな同梱 fn（`list.split_at[T]`、`list.iterate[T]`）を扱う。残作業は**カバレッジ**である。現在の走査範囲は狭い（同梱 stdlib を対象にする `CallTarget::Module` だけ）。次へ広げる：

- 最終的に Module に lower される `CallTarget::Method` + UFCS 形式。
- stdlib だけでなくユーザーパッケージ内の同梱 fn。
- モジュールをまたぐジェネリック連鎖（`stdlib/option.almd` に実際の同梱 fn を書き始めたとき、その中にある `list.iterate`）。

拡張した各経路を通る回帰テストを追加する。

<a id="why-ship-v0146-before-fixing-all-of-these"></a>

## すべてを直す前に v0.14.6 を出荷する理由

項目 (5) は完了している。(1)–(4) は MSR の説明を妨げない仕上げである。dojo ハーネスは、現行 dispatch がすでに扱える慣用的コードを書く。今 0.14.6 をリリースすれば、`develop` で段階的に掃除している間に、下流利用者は `cargo install --git --branch` からタグ付きバージョンへ移れる。

<a id="scope"></a>

## 範囲

0.14.7-phase3 の一連の作業の概算：

- (4) option/result シグネチャの正規化：2–3 時間
- (1) 統一 dispatch 入口 + (2) stub 廃止：合計 3–4 時間
- (3) ConcretizeTypes の強制的事後条件：2 時間（既知の穴を埋めて切り替える）
- (5) 拡張：dojo の証拠に基づき、拡張方向ごとに 1–2 時間

合計約 10 時間の集中的な作業。約 4 コミットに分けられる。
