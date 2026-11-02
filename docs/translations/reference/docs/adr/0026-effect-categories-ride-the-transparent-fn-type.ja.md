<a id="adr-0026-effect-categories-ride-the-transparent-fn-type--sets-flow-through-callbacks-unannotated-categories-form-a-two-level-hierarchy-and-inferred--manifest--host-is-one-chain"></a>

# ADR-0026：effect category を透過的な fn 型に載せる — 集合は注釈なしで callback を流れ、category は2階層を成し、inferred ⊆ manifest ⊆ host を一つの連鎖とする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md)
> 初期2026-10-03の状態とOpen節を、後の2026-10-25実装注記と併記する。異なる段階の記録であり、現行の単一完成判定ではない。static chainはruntime認可grantではない。過去の6/13category語彙と14leaf実装を統合せず、plain externのeffect-bit/ABI境界は未決として保持する。例示E0xxは出荷diagnostic IDではない。

- **状態**：Accepted（D1–D4、2026-10-03）。inference fixpoint（#3238）以外は未実装。§Open の問いは未決定。
- **日付**：2026-10-03
- **範囲**：effect bit と区別した effect *category* の意味、category set が function value/callback/closure を移動する方法、category の命名と grouping、compiler の見方と `almide.toml` の `[permissions]`、host が grant する capability の関係。
- **基礎**：[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（§D6：effect は fallible を含意。本 ADR で不変）、[ADR-0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md)（D2：裸の fn-type parameter は透過的。本 ADR は透過 bit を集合に拡張）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（pure `fn` 内の output/abort builtin）。
- **関連**：#3235（進行中の ADR-0025：private host capability としての wasm `process`。D2/D3 の `Proc` leaf の host 側）。
- **方針文書**：[effect-system-capability.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md)（Phase 4「EffectSet in Ty::Fn」は本 ADR が設計する型レベル作業。`IO` shorthand は D2 が置換）。
- **履歴**：2026-10-03。外部設計案「Almide effect仕様案 v0.1」（baseline 902e58d23）は、function type の上限として閉じた category set を求めた。判断前に言語横断比較（下記）を実施し、maintainer に3つの問いを一つずつ提示、すべて○。第4の診断については query command の形を tool 間で比較した後、○の回答を得た（D4）。

<a id="context"></a>

## 背景

develop（7f2f03cca）に存在するもの：

- **effect bit** は type checker が検査する。`fn` は `effect fn` を呼べず、fn 型引数は ADR-0009 に従い bit を持つ。
- **category** は別の、より粗い解析。`crates/almide-ir/src/effect.rs` には6個（`IO`、`Net`、`Env`、`Time`、`Rand`、`Fan`）あり、`pass_effect_inference.rs` は**module 名**で割り当てる（純粋な string 作業なのに `path` と `url` は `IO`/`Net`）。型には一切現れない。
- `src/cli/mod.rs::check_permissions` は推移的な集合を `[permissions].allow` と比較する。空 list はすべて許可（`src/project.rs:81`）で、未知の名は無言で捨てる。
- 推移閉包は20巡で止まっていたため、深い call chain は effectful root を clean と報告した。#3238 で修正（最小 fixpoint までの worklist）。
- `docs/wasm/capability-system.md` と roadmap は13 category を記述するが、code は6個。

したがって compiler は関数が外界に触れる*こと*を言えても、*どの部分か*を言えず、その部分に関する情報も callback を越えて残らない。

<a id="comparison-2026"></a>

### 比較（2026）

| 言語 | 表現 | 多相性、負担 | 粒度 | static ↔ runtime |
|---|---|---|---|---|
| Koka | row `<console,exn\|e>` | 暗黙 open row、row variable が error に漏れる | user-defined + handler | なし |
| Effekt | 暗黙に渡す capability | 文脈依存、effect variable を書かない、second-class function | user-defined + handler | なし |
| Flix | set `\ {FsRead, Net}`、Boolean/set 単一化 | `\ ef`、`ef1 + ef2`、`ef - Block` | 固定 base set + user-defined | package ごとの effect lock |
| Scala 3 capture checking | capability は値、`T^{io}` | capture set、重い、実験的 | capability | 言語内のみ |
| Roc | `->` pure / `=>` effectful | なし | 2値 | platform が effect を提供 |
| Wado | `with (Stdout, FileSystem)` | fn ごとに effect variable 1個、HOF 引数から推論。local は推論、`pub` は明示 | WASI interface と1:1 | component import |
| Aver | `! [Disk.readText]`、`! [Disk]` | — | method ごと、名前空間付き | runtime でも強制 |
| Jacquard | `->{net}` | row | handler | `--allow net`、`why-effect` command |
| Deno | 型にない | — | `--allow-read=path` | runtime のみ、理由を示さない |

最も近い設計は Wado（local inference、明示 `pub`、handler なし、component import への直接対応）。基礎語彙は Flix から handler を除いたもの。handler なしの閉集合は最先端に遅れるものではない。欠けていたのは package 境界の effect diff（Flix）、static effect から host grant への一つの連鎖（WASI、Jacquard）、経路付き診断（Jacquard）だった。

<a id="decision"></a>

## 決定

<a id="d1-a-bare-fn-type-parameter-is-transparent-for-the-category-set-too--2026-10-03"></a>

### D1。裸の fn 型引数は category set についても透過的（○ 2026-10-03）

ADR-0009 D2 はすでに、裸の `f: (A) -> B` が引数の effect *bit* を HOF call に渡す。同じ parameter が category *set* も渡すようになる。

```almide
fn apply(xs: List[String], f: (String) -> Int) -> List[Int] = xs |> list.map(f)
// apply(paths, (p) => count_lines(p)!)  — the call carries FS.read
// apply(names, (s) => string.len(s))     — the call carries nothing
```

- 書き手は effect variable を指定しない。surface language に `effects E` はなく、**diagnostic に variable を決して出さない**。`{FS.read}` のような具体集合だけ。
- 内部では checker が集合を解く（surface は union のみ。solver は Flix 形式の集合単一化を使ってよい）。裸の parameter と lambda で包んだ引数は同じ集合を生む。
- **明示的** slot（ADR-0009 D3、例 `effect (Req) -> Resp!`）は上限を固定する。保存・返却する function value は保存された値の集合を保ち、adapter が落としてはならない。

<a id="d2-categories-are-a-closed-two-level-hierarchy--2026-10-03"></a>

### D2。category は閉じた2階層（○ 2026-10-03）

| Parent | Leaf |
|---|---|
| `FS` | `FS.read`、`FS.write` |
| `Net` | `Net.fetch`、`Net.listen` |
| `IO` | `IO.stdin`、`IO.stdout`、`IO.stderr` |
| `Env` | `Env.read`、`Env.write` |
| — | `Proc`、`Time`、`Rand`、`Fan`、`Abort` |

- **親はこの4つのみ。** それ以外はすべて leaf。
- inference と `[permissions]` は leaf 単位。`pub` 境界は parent（`{FS}`）を粗い上限として宣言でき、後で内部に `FS.write` を追加しても interface change にならない。
- parent は記述時の registry version の leaf を意味する。**後から追加した leaf は既存 parent に自動参加しない**。明示 grant が必要。
- category は module でなく**operation ごと**に付ける。`url.parse` と path 操作は空、`http.get` は `Net.fetch`。
- `Abort` は `panic` / 失敗した `assert*` / `process.exit` のみ。`Result` error は `Abort` でなく、fallibility は ADR-0002 の別軸のまま。`panic` は `Abort` と `IO.stderr` の両方を持つ。
- roadmap の `IO = FS.read + FS.write + IO.std*` shorthand を置換。`IO` は標準3 stream、file access は `FS`。

<a id="d3-inferred--manifest--host-is-one-chain--2026-10-03"></a>

### D3。inferred ⊆ manifest ⊆ host は一つの連鎖（○ 2026-10-03）

```
inferred set   ⊆   [permissions].allow   ⊆   capabilities the host grants
(compiler)         (almide.toml)              (wasm imports / Porta)
```

- **wasm**：inferred set 外の import を artifact から prune し、grant されない operation は単に禁止でなく存在しない。
- **public API**：`pub` function の effect 上限拡大は `scripts/check-interface-diff.sh` が **breaking** と分類し、既存 `@deprecated` / epoch 手順を通す。
- **依存**（後段）：各依存の effect summary を lock に記録し、summary が増えた版への更新には明示 acknowledgment を要求する。
- 順序：pruning と interface diff を先に、lock は最後。
- static chain は runtime grant ではない。run が触れられる file/host/process は host の判断のまま。effect summary は決して認可 token ではない。

<a id="d4-a-violation-names-its-path-the-query-is-almide-check---effects--2026-10-03"></a>

### D4。違反は経路を示し、query は `almide check --effects`（○ 2026-10-03）

```
error[E0xx]: `validate` may write files, but its bound is {FS.read}
  --> src/check.almd:12:1
   = added: FS.write
   = path:  validate → normalize → save_cache → fs.write_text   (src/cache.almd:40:3)
   = hint:  1. return the data and let the caller save it
            2. move `save_cache(...)` out of `normalize` into the caller
            3. if writing is intended, widen the bound to {FS.read, FS.write}
```

（code と文面は例示であり、error code は実装時に割り当てる。）

- 先頭行は境界と追加 category を示す。`path:` は category ごとに代表経路を一つ示し、operation の位置で終わる。
- callback を通る場合、既知なら具体 callee、そうでなければ parameter（`apply → f (arg 2 of apply)`）を経路に示す。callee を捏造しない。
- fix-it の順は*値として受け取る／caller へ移す／境界を広げる*。拡大は最後で、`almide fix` は bound や `[permissions]` に category を追加しない。diagnostic は agent が自身に権限を与える近道を提供しない。
- **query は新 verb でなく既存 view を拡張する**。

  ```
  almide check --effects              # every function's set (today's output)
  almide check --effects FS.write     # functions carrying FS.write, with every path
  almide check --effects validate     # validate's categories, with a path for each
  almide check --effects FS.write --json
  ```

  category（`FS.write`、大文字開始・dot 付き）と function 名は同じ表記にできないので、引数は一意。`--json` は `--effects` と組み合わせられ（当時は競合）、出力順は安定し、同じ report を MCP tool とする。diagnostic の `path:` 行と `--effects` 出力行は同じ形なので、読者は読み替えなしで error から query へ移れる。
- 形の先例は、新しい top-level verb でなく `cargo tree -i`（既存 view への新しい問い方）。却下したのは hyphen 付き top-level `why-effect`（Jacquard の `jac why-effect`。Almide の verb は1語）、一般的 `almide why`（1用途では一般 verb を正当化しない）、名詞 command `almide effects`（同じ集合を2箇所で一覧にする）。ほかに検討した先例：`go mod why`、`npm explain` / `yarn why`、`nix why-depends A B`、Gradle `dependencyInsight`、Bazel `somepath`。

<a id="rejected"></a>

## 却下

1. **user-defined effect と handler**（Koka、Effekt、Unison、OCaml 5、Flix）。multi-shot handler は reference counting/region と衝突し、想定する読者から control flow を隠す。将来追加するなら新 ADR で one-shot のみ。
2. **surface effect variable**（`(A -> B) effects E`、Koka row、Rust `?effect`）。ADR-0009 は error message を理由に row polymorphism をすでに却下。D1 はその構文なしで同じ表現力を得る。
3. **second-class function**（Effekt）。既存 code が使う closure の保存と返却を壊す。
4. **surface 上の値としての capability**（Scala capture checking）。注釈が重くまだ実験的。D3 は代わりに host 境界へ考えを適用する。
5. **全 function/closure への必須 effect annotation**（Vera）と**method 単位の粒度**（Aver）。負担が大きすぎ／細かすぎる。選んだ地点は4 parent の下に14 leaf。
6. **型に effect がないこと**（OCaml 5）と **runtime-only permission**（Deno）。どちらも理由と経路を言えない。

<a id="open--not-decided-by-this-adr"></a>

## Open — 本 ADR では未決定

- **category 語彙の統一**：`almide check --profile critical --allow` は `IO, Net, Env, Time, Rand, Process` を受け取る。`effect.rs` の6つ、roadmap の13と並ぶ第3の表記（`Process` 対 `Proc`）。D2 の versioned registry が3つ全部を置換する必要がある。
- 宣言上限と明示的空集合（提案の `effects {}`）の **surface syntax**。
- **plain `fn` の互換 bound**：ADR-0022 は output/abort builtin を認めるため、plain `fn` は最大 `{IO.stdout, IO.stderr, Abort}` であって `{}` ではない。厳密な空 bound を提供するか、その綴りは何か。
- **manifest versioning**：当時空の `allow` は「全部」を意味する。versioned manifest は省略／空／全部を区別し、未知名を拒否（提案 EFF 03、EFF 18）しつつ、当時の意味を予告なしで反転してはならない。
- **言語定義 trap**（overflow、bounds）と、それが `Abort` かどうか。
- **extern と native-shim の契約**、trusted dependency の報告方法（提案 EFF 14、EFF 15）。

<a id="consequences"></a>

## 帰結

- `Ty::Fn` に effect bit と並ぶ category set を追加（roadmap Phase 4 の設計上の障害は解消、日程未定）。checking 後は category を消去し、monomorphisation が集合によって増殖しないことを意図する（未検証）。
- category 割り当てを module 名から operation 単位契約の単一 versioned registry へ移し、checker、manifest validation、diagnostic、docs がそこを読む。
- 当時の CLI 名（file を含む `IO`）には D2 名への移行経路が必要。

<a id="falsifier"></a>

## 反証条件

1. **Dojo で透過集合が明示 bound より LLM 編集失敗を増やす**（例：編集していない call site に effect が「現れる」）。`pub` HOF の明示 slot へ後退する。
2. **書き手がどこでも parent を使い leaf を絞らず**、`pub` bound が情報を持たなくなる。parent を manifest に限定する。
3. **import pruning または interface-diff 分類が release に誤った breaking 判定を出し**、reviewer の override が必要になる。連鎖が誤ったものを検査しており、仕様を定義し直す。

<a id="references"></a>

## 参考資料

- Flix, effect 多相性: https://doc.flix.dev/effect-polymorphism.html
- Flix, "Fighting Supply Chain Attacks with Effect Systems" (OOPSLA 2026):
  https://2026.splashcon.org/details/oopsla-2026/120/Fighting-Supply-Chain-Attacks-with-Effect-Systems
- Scala 3 capture checking:
  https://scala-lang.org/api/3.x/docs/experimental/capture-checking/overview.html
- Roc functions: https://roc-lang.org/docs/main/langref/functions/
- ローカル： `../almide-references/wado/docs/spec.md` (Effect System),
  `../almide-references/aver/docs/language.md`,
  `../almide-references/jacquard/README.md`,
  `../almide-references/research-2026/agent-verify-memory.md`
- 内部： #3238, `crates/almide-ir/src/effect.rs`,
  `crates/almide-codegen/src/pass_effect_inference.rs`,
  `src/cli/mod.rs::check_permissions`, `docs/wasm/capability-system.md`


<a id="implementation-note--categories-through-stdlib-callbacks-and-fn-values-4152-2026-10-25"></a>

## 実装注記 — stdlib callback と fn value を通る category（#4152、2026-10-25）

D1 の集合は stdlib 高階呼び出しを越えて残らなかった。`@intrinsic` function（`list.map`、`= _`）を空本体として走査していたため、`list.map(paths, fs.read_text)` の `IO` は誰にも計上されず、`Time` だけを許す manifest で build して file を読めた（#4152）。現在の記録では：

- body のない stdlib function は各 fn 型引数を実行し（D1 の透過 parameter として call site に計上）、何も保持しない。`stdlib/` の全 `@intrinsic` HOF と照合し、返すものに callback を保存するものはなかった。type-variable parameter で受ける値は引数の具体型で pool し、結果は call の型で pool から読む。stdlib 本体を lowering しない `almide check` も宣言から同じ事実を読む（`effect::stdlib_param_uses`）。
- 値として取り出した stdlib fn は category（`fs.read_text` → `IO`）を持ち、`effect fn` なら effect bit も持つ（ADR-0009、E109）。
- 逃げた値は arity でなく signature ごとに pool する（alias 展開済み、carrier form の `effect` slot）。generic body は type-variable 値を再 pool せず、型が signature を open にする read は alias し得る全 pool を読む。これにより `(Handler) -> Handler` の pure middleware chain に全 handler の費用を計上しない。
- stdlib function の category は宣言でなく使用箇所に計上するため、`[permissions]` は未使用の bodyless declaration を判定しない。

§Open は不変。6 category 語彙、宣言 bound の構文、plain-fn bound、manifest versioning はここでは決めない。

<a id="implementation-note--d2s-leaves-4155-2026-10-25"></a>

## 実装注記 — D2 の leaf（#4155、2026-10-25）

`process` は `Env` と分類されていたため、`$HOME` 読み取りに `Env` を与える manifest が任意 command も開始できた。D2 を記述どおり実装する。

- `Effect` は14 leaf。`[permissions].allow` は leaf または4 parent（`FS`、`Net`、`IO`、`Env`）を受け取り、列挙された leaf を grant する。`Proc` はいずれの parent にも属さない。
- operation 単位：`STDLIB_MODULE_EFFECTS` の module 行を既定とし、異なる operation の function は自身の宣言に `@category("FS.write")` を書く（`fs.write_text`、`env.set`、`http.serve`、`process.args` など）。
- output/abort builtin は leaf を持つ（`println` は `IO.stdout`、`eprintln` は `IO.stderr`、`panic` と失敗 `assert*` は `Abort` + `IO.stderr`、`process.exit` は `Abort`）。D2 は inference と `[permissions]` が leaf 単位と規定し、§Open の「plain fn bound」は宣言 bound の話で、manifest grant の話ではない。
- `[permissions]` は program 自身の function（entry と user module）を判定。stdlib function は計上先 caller を通して判定するので、`list.range_step` 内の未使用 `panic` は何も拒否しない。

D2/§Open が定めていない点は保守的に選ぶ。

1. **開いた socket または shared-memory region の operation**（`net.tcp_read`、`unix_send`、`shm_write` など）は `Net.fetch` と `Net.listen` の両方を持つ。handle からどちらが開いたか分からないため。
2. **`proc = [...]` list なしの `Proc`** は全経路で warning、`--profile critical` で error。
3. **旧名も parse する**（`IO`、`Env`、`Net` は parent）が、意味する範囲は狭まる。旧意味で覆われた leaf の拒否には migration note を出す。dialect-epoch-15 break に記録する。
4. **ここでは不変（§Open、語彙統一）**：`--profile critical --allow` はまだ `IO, Net, Env, Time, Rand, Process` を受け取る。

<a id="implementation-note--dispatch-plain-host-fns-routes-4158-4159-2026-10-25"></a>

## 実装注記 — dispatch、plain host fn、経路（#4158、#4159、2026-10-25）

- **protocol dispatch（#4158）。** bound 経由の `effect` protocol method を pure fn から呼ぶと E006。すべての protocol で implementation の effect 指定は protocol 宣言と一致する必要があり、`==` や derive が暗黙に呼ぶ convention も含む。よって pure caller は effect impl に dispatch せず、集合に dispatch edge は不要。
- **plain host fn（#4159）。** host に作用する plain `fn` は `effect fn` になった。gate は `All(_)` module の public plain fn を、`@category()` 宣言がない限り拒否する（host に到達しないもの：`random` の seeded generator、`env.os`）。あらゆる plain fn 上の host category も拒否する。`env.millis` / `unix_timestamp` / `sleep_ms` は `Time`。
- **変更なし — plain-`fn` `@extern`（#4159、判断が必要）。** `[permissions]` では全 category（⊤）を持つままだが、E006 も effect bit もその call を effect と扱わない。JS host の同期 import（`spec/wasm_host_js`）は pure fn から呼ぶ plain `@extern` fn で、その export wrapper を host が plain value として呼ぶ。それらの call を effect にすると export ABI が変わる。保守的な代案、plain `@extern` の拒否または call の effect 扱いは両方ともこのモデルに breaking であり、扱う場所は ADR-0027 §5（extern の bound）。
- **経路。** `[permissions]` は entry file の project（その位置または上位で最も近い `almide.toml`）から取得し、`check` は全 user module を判定、`almide test` の両 lane と `emit` も適用する。

> コード内コメント・例示の訳（コード原文は保持）：apply(paths, …) は FS.read を持ち、apply(names, …) は何も持たない。集合の図は compiler の推論集合 ⊆ almide.toml の許可集合 ⊆ host が grant する capability（wasm import / Porta）。診断例は validate が FS.read 上限に対しファイルを書き得ること、追加FS.writeとvalidate→normalize→save_cache→fs.write_textの経路を示す。hintはデータを返しcallerに保存させる、save_cacheをcallerへ移す、意図的なら上限をFS.read/FS.writeへ広げる、の順。query例のコメントは全関数の集合（当時の出力）、FS.writeを持つ関数と全経路、validateのcategoryと各経路。
