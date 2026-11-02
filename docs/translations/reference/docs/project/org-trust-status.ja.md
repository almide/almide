<a id="almide-org--legacy-v1-trust-spine-report"></a>

# Almide org — legacy v1 trust-spine 報告

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](org-trust-status.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](org-trust-status.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](org-trust-status.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/org-trust-status.md)
> この reader は元 generator の入力でも出力先でもなく、自動更新しない。原文の途中で切れた診断文字列、列不整合、過去の状態の差を修復や再測定せず保持する。特に表と後段注記／spot-check は porta の件数や複数 repo の verdict が異なる。旧 host の wasm 非対応という除外は歴史資料であり、現行 capability の案内ではない。

> 元の generator：`scripts/org-trust-status.sh`。元の org directory：`/Users/o6lvl4/workspace/github.com/almide`。

> **過去の証拠の境界。** 保持した verdict と 2026 年 7 月の spot-check は歴史的観測であり、現在の organization の状態ではない。script は legacy MIR classifier と 2026-07-02 固定の `BYTE_VERIFIED` 集合を組み合わせる。再実行してもそれらの label を再テストしない。現在の Wasm 経路は structural で、下の除外条件を記述して以降 host-operation 対応は変わっている。独立した test-suite 合格が確立するのはそこでの assertion であり、任意の stdout/stderr/exit の byte identity ではない。
>
> 新たな org 全体の verdict には、compiler と sibling の revision 固定、現行 classifier、明示的な挙動比較が必要。これらの元の行は、測定していない現行 verdict に置き換えず保存する。

**14/28 repo が RESOLVED v1 pipeline で完全に render される**（全 `src/` module が canonical sibling + dep 解決の下で wall-free に lowering される。`almide build --target wasm` と同じ経路。26 module にまだ wall があり、5 は frontend が拒否）。補助的な関数単位 classify sweep：19/27 repo が wall=0、real wall = 21 / native-FFI = 46（除外）、2433 関数を lowering。

> **見出しは resolved 指標。** classify_corpus は module 間解決なしで 1 file を読むので、file ごとの数値は実 pipeline が解決する artifact（`lay.*`/`v.*` reference）も数える。verdict ではなく関数ごとの wall-reason に対処する手掛かりとして使う。

> **wall=0 指標 = REAL lowering wall のみ。** NATIVE-FFI wall は `@extern(rust/rs)` 宣言（wasm 形式なし）、または恒久的に wasm 非対応の stdlib effect（`process.exec/exit/run`、`http.request`、`net.*`）を推移的に呼ぶ関数。これらは native host を必要とするため wasm に決して lowering できず、既存 `@extern(wasm)` WASI import と同じく除外する。lowering bug ではない。wall=0 目標を動かすのは REAL wall だけ。

⚠ **wall=0 ≠ 正しい。** 全関数が v1 lowering subset 内という意味であり、出力の挙動が正しいという意味ではない。正本の ② gate は **native==wasm の byte-match**（repo 自身の vector/test を native と `--target wasm` の両方で実行）。`🟡 lowers, byte-match TODO` は wall=0 に達したが byte 検証はまだの repo を示し、まさに無言の誤コンパイルが隠れ得る箇所である（例えば RFC vector が見つけるまで初期 sha1=0 を偽装した `var v=w` scalar-aliasing bug）。

<a id="per-repo-sorted-by-walls"></a>

## Repo ごと（wall 順）

| repo | 解決済み（render） | lowering 数 | real wall | native-FFI | classify 状態 | 最多 wall 理由 |
|------|--------------------|-------:|-----------:|-----------:|-----------------|-----------------|
| `homullus` | 🔴 4/11 — frontend: type errors: [\"check() expects 3 argument(s) but got 2\" | 77 | 6 | 4 | 🔴 6 +1 xmod, 4 native-FFI 除外 | REAL heap-result X outside the executable subset cannot  |
| `ceangal` | 🔴 8/10 — [render_program] Unsupported("parse error: error: Expect | 250 | 5 | 0 | 🔴 5 | REAL use of unbound var VarId(N) |
| `toml` | 🔴 0/1 —   parse_octal: Unsupported("guard-else early return cann | 55 | 3 | 0 | 🔴 3 | REAL guard-else early return cannot be faithfully lowere |
| `porta` | 🔴 10/18 —   dedup_env: Unsupported("while body with a heap-accumul | 165 | 2 | 24 | 🔴 2 +2 xmod, 24 native-FFI 除外 | NATIVE-FFI heap move-out from <other> (only a bound var, |
| `almide-dojo` | 🔴 5/7 —   compile_and_test: Unsupported("heap-result X outside t | 84 | 2 | 1 | 🔴 2, 1 native-FFI 除外 | REAL heap-result X outside the executable subset cannot  |
| `wasm-canvas` | 🔴 1/2 —   main: Unsupported("non-empty List[heap] literal with n | 46 | 1 | 0 | 🔴 1 | REAL non-empty List[heap] literal with nested-ownership  |
| `canvas` | 🔴 1/2 —   main: Unsupported("non-empty List[heap] literal with n | 46 | 1 | 0 | 🔴 1 | REAL non-empty List[heap] literal with nested-ownership  |
| `almai` | 🔴 8/10 —   retry_loop: Unsupported("heap-result X outside the exe | 183 | 1 | 2 | 🔴 1, 2 native-FFI 除外 | NATIVE-FFI heap-result X outside the executable subset c |
| `yaml` | ✅ 1/1 | 74 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `wasm-webgl` | ✅ 3/3 | 85 | 0 | 0 | 🟡 0 — lowering 可、byte-match TODO | — |
| `vscode-almide` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `tree-sitter-almide` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `three` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `tech-article` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `syschord` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `svg` | ✅ 2/2 | 80 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `sha1` | ✅ 1/1 | 14 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `rsa` | ✅ 2/2 | 19 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `prism-ml-llama.cpp` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `playground` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `obsid` | 🔴 3/4 — [render_program] Unsupported("unlinked stdlib/runtime ca | 177 | 0 | 0 | 🟡 0 — lowering 可、byte-match TODO | — |
| `nn` | 🔴 13/15 —   approx_row: Unsupported("scalar tail outside the value | 289 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `lumen` | ✅ 4/4 | 75 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `docs` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `csv` | ✅ 1/1 | 10 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `bonsai-almide` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `bigint` | ✅ 1/1 | 35 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `base64` | 🟠 0/1 — frontend: type errors: [\"cannot call effect function 'decode_chunk | 0 | 0 | 0 | ▫ すべて module 間 | — |
| `audio-poc` | ✅ 1/1 | 4 | 0 | 0 | 🟡 0 — lowering 可、byte-match TODO | — |
| `almide-web` | ✅ 5/5 | 23 | 0 | 0 | 🟡 0 — lowering 可、byte-match TODO | — |
| `almide-wasm-bindgen` | 🔴 0/1 —   tuple_supported: Unsupported("scalar tail outside the  | 97 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `almide-sqlite` | 🔴 0/1 —   open: Unsupported("scalar tail outside the value subse | 14 | 0 | 14 | 🟡 0 — lowering 可、byte-match TODO, 14 native-FFI 除外 | — |
| `almide-lander` | 🔴 1/2 —   main: Unsupported("effectful/impure stdlib Module call | 10 | 0 | 1 | ✅ 0 — byte 検証済み, 1 native-FFI 除外 | — |
| `almide-js` | — | — | — | ▫ Almide の `src/` library ではない | — |
| `almide-grammar` | ✅ 2/2 | 17 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `almide-bindgen` | ✅ 23/23 | 405 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `almide-aituber` | ✅ 8/8 | 72 | 0 | 0 | 🟡 0 — lowering 可、byte-match TODO | — |
| `aes` | ✅ 1/1 | 27 | 0 | 0 | ✅ 0 — byte 検証済み | — |
| `_references` | — | — | — | ▫ Almide の `src/` library ではない | — |

<a id="cross-repo-wall-lever-most-frequent-reasons"></a>

## Repo 横断の wall 対策（最多の理由）

上位の理由を解消する一つの brick は、複数 repo を一度に前進させる。

| 件数 | wall 理由（正規化、元の診断断片） |
|------:|--------------------------|
| 29 | NATIVE-FFI heap move-out from <other> (only a bound var, fresh literal, or call) not in this 
| 8 | REAL heap-result X outside the executable subset cannot be faithfully returned in this brick (
| 7 | NATIVE-FFI heap-result X outside the executable subset cannot be faithfully returned in this b
| 3 | NATIVE-FFI heap-result Try cannot be faithfully returned in this brick (would move out an empt
| 3 | NATIVE-FFI effectful/impure stdlib Module call process.exec needs a declared capability not in
| 2 | REAL use of unbound var VarId(N) |
| 2 | REAL non-empty List[heap] literal with nested-ownership elements (a heap-field record/tuple, a
| 2 | REAL List argument cannot be faithfully materialized in this brick (would borrow an empty defe
| 2 | REAL guard-else early return cannot be faithfully lowered (vN has no early-return             
| 2 | NATIVE-FFI match over an UNTRACKED subject with a call-bearing arm cannot take the both-arms l
| 1 | REAL while body with a heap-accumulator reassignment cannot be faithfully lowered (the model-o
| 1 | REAL if over an unresolvable condition (Gt over Float; Var vs LitFloat) with a call-bearing ar

<a id="notes"></a>

## 注記

- entry だけでなく**全** `src/*.almd` module を sweep する。**resolved 列**は各 module を実 pipeline（sibling + dep 解決）で render するため、module 間 importer もそこで測定する。classify_corpus は module 間 import 解決なしに 1 file を読むので、sibling importer を skip し、repo ごとに `+N xmod` と表示する。関数ごとの wall reason のためだけに使う。
- `almide` repo 自体は参照 corpus（自身の `proofs/corpus-wall.sh` gate）であり、ここでの対象ではない。
- `porta` は NATIVE HOST（`almide.toml`：wasmtime + reqwest/Net）。完全な MCP server は設計上 native 専用（WASI preview1 に net はなく wasmtime を embed できない）であり、25 native-FFI は host call、v1 subset にあるのは PORTABLE protocol layer（jsonrpc/config）のみ。byte-verification は両 target の全 test suite（wasm 実行可能な 7/7 file + 設計上 native 専用の FFI 1 file）と、完全 MCP server の native build の両方が green という意味。
- ✅ **byte-verified** = repo 自身の test suite が `almide test --target native` と `--target wasm` の両方で完全合格すること（assertion が vector）。2026-07-02 sweep は、そこで見つかった wasm bug 6 分類を修正した後、これを 2 repo（yaml、sha1）から test suite を持つ全 repo へ拡大した（pass-through/copied-pair 経路の share/+1、list-layout 割り当て不足、Value deep-eq 欠落、value.merge oracle mismatch、bytes.set value semantics。contract C-121..C-125、docs/roadmap/active/v1-org-byte-verification.md 参照）。
- まだ byte 検証できない repo：`almide-web` / `almide-sqlite`（test vector なし。まず suite を書く）、`almide-dojo`（task-bank fixture であり compile 可能な suite ではない）。
- repo を byte 検証するには、自身の test を両 target（`almide test --target native` と `--target wasm`）で実行し、それぞれ完全合格を要求してから script の `BYTE_VERIFIED` に追加する。

<a id="graphics--ai-stack--production-target-spot-check-2026-07-02"></a>

## Graphics / AI stack — 製品 target の spot-check（2026-07-02）

graphics と AI repo の大部分は BROWSER-HOSTED wasm app（JS host が module を load）なので、それらの「v1 で動く」は、当時の compiler が有効な wasm artifact を生成するという意味。rendering host に headless byte-run 比較は当てはまらない。検証した状態：

| repo | 種類 | 状態 |
|------|------|-------|
| `svg` | graphics（pure lib） | ✅ byte 検証済み （suite、両 target）; module 間の `render(group(..))` compiler stack overflow は修正済み（2026-07-03 equi-recursive unify guard） |
| `lumen` | graphics（pure lib） | ✅ byte 検証済み （suite、両 target） |
| `canvas` | browser-hosted app | ✅ wasm build 正常 |
| `wasm-canvas` | browser-hosted app | ✅ wasm build 正常 |
| `wasm-webgl` | browser-hosted app | ✅ wasm build 正常 |
| `obsid` | browser+native-hosted app | ✅ wasm build 正常 （test suite はまだない） |
| `almide-aituber` | browser-hosted app | ✅ wasm build 正常 （v1 の差異は develop 側の #717 auto-?-branch-retype 修正の欠落による。2026-07-02 に cherry-pick） |
| `homullus` | AI agent | ✅ byte 検証済み （suite、両 target） |
| `almide-sqlite` | native host lib | ✅ suite green（10 tests、:memory: + file 永続化）。native host package（rusqlite FFI）、2026-07-03 検証 |
| `almide-web` | browser bindings | ✅ headless vector green（17 行 byte-match：全 import surface + intern protocol + runtime/headless.mjs 下の callback）。2026-07-03 検証 |
| `nn` | AI（neural nets） | ✅ byte 検証済み （suite：13/13 files、両 target）。2026-07-02 に TCO-temp type 修正、nested-HOF lambda-pin 修正（C-126）、Matrix repr、SIMD fast-exp clamp、unwrap_or chain 修正（C-127）で障害解消 |
| `almai` | AI（LLM client lib） | ✅ native suite green（56 tests、2 files）。2026-07-03 の STRUCTURAL-TWIN merge で解決：checker が module 間の同 base-name・同 shape の record 宣言を統合し、codegen が一つの canonical struct へまとめる（flatten twin-merge。E0063 default-fill と bare-ref 修復も同じ機構を使う）。wasm leg：native fallback（`env.get`/http は native 専用。LLM client は network host を必要とする） |

以上の全 graphics/AI repo は、この記録では v1 で build または検証できる。

> 表の診断欄は、途中で切れた実際の出力文字列を原文どおり残している。診断語句の訳：frontend: type errors は「フロントエンドの型エラー」、check() expects 3 argument(s) but got 2 は「check() は引数3個を要求するが2個だった」、parse error は「構文解析エラー」、unlinked stdlib/runtime call は「未リンクの標準ライブラリ／ランタイム呼び出し」、scalar tail outside the value subset は「値 subset 外の scalar tail」、cannot call effect function は「effect 関数を呼べない」。省略された末尾は補っていない。
>
> wall reason の訳（行順）：29 件＝許可形（束縛変数・新規 literal・call）以外の heap move-out。8 件＝実行可能 subset 外の heap-result X を忠実に返せない。7 件＝同じ heap-result X の NATIVE-FFI 分類。3 件＝heap-result Try を忠実に返せず、空の何かを move-out するという途中で切れた理由。3 件＝process.exec の effectful/impure stdlib Module call に未収録の宣言 capability が必要。2 件＝未束縛 VarId(N) の使用。2 件＝所有権が入れ子の要素を持つ非空 List[heap] literal。2 件＝List 引数を忠実に具体化できず、空の何かを借用するという途中で切れた理由。2 件＝guard-else early return を忠実に lowering できず、vN に early-return がない。2 件＝UNTRACKED 対象の match と call を持つ arm が、両 arm の何かを取れないという途中で切れた理由。1 件＝heap-accumulator 再代入を持つ while 本体を忠実に lowering できない。1 件＝解決不能な条件（Float の Gt、Var 対 LitFloat）の if と call を持つ arm に関する途中で切れた理由。REAL/NATIVE-FFI の分類と数値は元表を正本とする。
