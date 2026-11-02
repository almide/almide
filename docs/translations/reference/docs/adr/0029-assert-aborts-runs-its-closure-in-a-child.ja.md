<a id="adr-0029-testingassert_abortsexpected-closure-runs-only-its-closure-in-a-child-process-or-a-fresh-instance-the-closure-captures-nothing-every-abort-counts-and-testingassert_throws-leaves-at-dialect-epoch-14"></a>

# ADR-0029: `testing.assert_aborts(expected, closure)`は子processまたは新instanceでclosureだけを実行する。captureなし、全abort対象とし、`testing.assert_throws`はdialect epoch 14で廃止

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.zh-CN.md)

> 非規範的な歴史読者スナップショット。固定原本不変更・正本。自動同期・独立テスト・新しいprocess実行許可・現行release保証ではない。採択済/9問決定済と前段の裁定待ち文の原文矛盾を保持する。D3のspec4call見積りとD7の追加caseも勝手に再集計しない。定義済みabortはstatus1＋指定stderr headerであり、全終了原因ではない。raw panic/trap/signalの除外と意図的header＋exitの偽陽性を保持。子のeffectは継承環境/fileへ作用でき、program状態分離はsecurity sandboxではない。stock-wasmtime fallbackとinterp棄権を明示。診断/契約IDは当時の予約で実装時再確認が必要。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md)

- **状態**: 採択（2026-10-18）。9つのOpen項目はQ15が選んだSwift exit-test先例に従い決定。[未決項目](#open)参照。
  - 方向は2026-10-17のowner裁定（○）: `testing.assert_throws(f, expected)`を置換しepoch 14で導入。
  - 綴りはowner Q15（2026-10-17）: 本体内call `testing.assert_aborts("expected substring", () => { … })`。Swift 6.2 exit test同様、runnerがclosureだけを子process（native）または新instance（wasm）で実行。capture不可、全abortを対象とする。
  - Q15は初稿のtest属性`@aborts("…")`、次のtest-header modifierを却下。下に却下案として残す。
  - 本ADRはrunner、capture検査、移行を補う。[未決項目](#open)の問いにはまだ裁定が必要。
- **日付**: 2026-10-17
- **範囲**:
  - `testing.assert_aborts`の表面と型。
  - 呼出し場所とclosure captureなしのcheck時規則。
  - native test binaryとwasm test laneがclosureを隔離実行する方法、親の比較対象。
  - `testing.assert_throws`削除。
  - 契約/dialect台帳変更。
- **番号**: ADR-0028は未mergeの`fan-semantics`（選択名）で使用中なので0029。
- **関連**: [ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.2（E095、served appも別instance実行ゆえlocal capture不可）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)、[ADR-0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)（private embedded-host import）、[ADR-0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md)、[ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) Open3（本ADRで置換）、C-067 / C-153 / C-196 / C-219 / C-350 / C-381 / ALS-T6 / ALS-T18、#3747、`proofs/dialect-epochs.toml` epoch14（branch `remove-33`）。

<a id="context"></a>

## 背景

`testing.assert_throws(f, expected)`はthunk `f`を実行し、`f`が`expected`を含むmessageでpanicするとpassする。native test buildという1箇所だけで動き、そこでも見かけより捕捉範囲が狭い。

**Native。**
- `runtime/rs/src/testing.rs`が`f`を`std::panic::catch_unwind`で囲む。
- `--test` buildは`panic(msg)`をunwindするRust `panic!`のままにする（`almide_panic!`の`cfg!(test)` branch、`crates/almide-codegen/src/lib.rs:414-422`）。ゆえに`panic`は捕捉できる。
- 他のabortは全て`almide_abort`経由で、一度だけのguard付き`process::exit(1)`。範囲外index（C-067）、ゼロ除算/overflow（ALS-T6）、stack overflow（C-196）、domain errorが該当。`catch_unwind`はexitを見られない。
- それらが`assert_throws`内で起こるとtest binary全体が終了。testはpassせず、同fileの後続testも走らない（`src/cli/test_output.rs`、`a_process_ending_abort_leaves_the_failing_test_unfinished`）。

**Wasm。**
- unwindできず、全abortはmodule内stderr行と`proc_exit(1)`。
- `crates/almide-mir/src/host_ops.rs:243`は`testing.assert_throws`をdecline（「MIR rungが形式を持たないunwind catchが必要」）。
- 使用する2 test fileは`// wasm:skip`付き。`proofs/wasm-fallback-baseline.txt`（`HARNESS_LIMITATION`）、`tests/wasm_skip_ledger_test.rs`（`NoUnwinding`）に載り、native fallbackだけで走る。

**Test block外。** direct callはT18 abortへdesugar（C-381、`crates/almide-frontend/src/lower/calls_testing_assert.rs`）。何も捕捉できず必ず失敗する。

関数はabortを捕捉してmessage検査すると約束するが、一般ケースで守るlegがない。process内で捕捉できなければ、別process/instanceをabortさせ外から観測するしかない。Swift 6.2はまさにこれをtest libraryへ組み込む。ownerはその形を選んだ。

<a id="every-current-caller"></a>

### 当時の全呼出し元

| 場所 | 内容 | Repo |
|---|---|---|
| `spec/stdlib/coverage_assert_throws_test.almd` | 2 test: `panic("oh no")` / `"oh no"`と`panic("something went wrong")` / `"went wrong"`; `// wasm:skip` | almide, als |
| `spec/stdlib/testing_polarity_test.almd` | 1 testに**2 call**: `assert_none(some(1))`と`assert_err(ok(0))`; `// wasm:skip` | almide, als |
| `spec/wasm_cross/testing_assert_abort_throws.almd` | C-381証拠: test外call、常に失敗 | almide, als |
| `stdlib/testing.almd:10-11` | `@intrinsic`宣言 | almide |
| `runtime/rs/src/testing.rs:35-49` | `almide_rt_test_assert_throws` | almide |
| `crates/almide-frontend/src/lower/calls_testing_assert.rs` | T18 desugar分岐（`"assert_throws" => Some(2)`、thunk処理） | almide |
| `crates/almide-mir/src/host_ops.rs:243`, `crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs:103-139` | wasm decline行とcorpus classifier | almide |
| `crates/almide-wasm/src/testing.rs:19` | コメント | almide |
| generated: `crates/almide-types/src/generated/embedded.rs`, `crates/almide-codegen/src/generated/{rust_runtime.rs,runtime_fn_modes.rs}` | 上記から再生成 | almide |
| `proofs/stdlib-inspection.toml:11101`, `proofs/impl-count-ledger.toml:5938`, `proofs/target-availability.toml:126,280`, `proofs/wasm-fallback-baseline.txt:17,34,35` | 台帳行 | almide |
| `tests/wasm_skip_ledger_test.rs:50,80`, `tests/wasm_dispatch_coverage_test.rs:28` | skip台帳とコメント | almide |
| `crates/almide-spine/tests/golden/spec-check-manifest.txt:612`, `crates/almide-syntax/tests/golden/spec-ast-manifest.txt:612` | fixture manifest | almide |
| `docs/stdlib/testing.md:5-13,131`, `docs/stdlib/since.toml:941`, `docs/stdlib/semantics-manifest.toml:2351`, `docs/diagnostics/E081.md:29`, `docs/specs/als/text-and-numbers.md:301-309`（mirror） | 文書 | almide |
| `ref/src/stdlib_testing.rs:22` | 参照evaluatorのtesting list | als |
| `docs/contracts/contracts.toml` C-381, `docs/specs/als/text-and-numbers.md` ALS-T18, `docs/contracts/conformance.md:140` | 契約とspec | als（規範）、almide（mirror） |

`CHANGELOG.md:492`と`llms.txt:380`は履歴として言及するだけで変更しない。

<a id="survey-how-languages-say-this-code-must-abort-with-x"></a>

## 調査: 「このcodeはXでabortすべき」を各言語でどう表すか

| 言語 | 綴り | レベル | Message照合 | 捕捉不能abort（`exit`、fatal error） |
|---|---|---|---|---|
| **Swift（Testing、6.2）** | `await #expect(processExitsWith: .failure) { … }` | 本体、closure | exit条件、子出力も検査可 | **子process**: 現processと同じ実行fileで新process開始、`expression`は親で呼ばない。closureは親processや外側lexical context由来の状態をcapture不可（compile時） |
| Go | 組込なし。test binaryを子として再実行する慣用法 `exec.Command(os.Args[0], "-test.run=TestCrasher")`、`BE_CRASHER=1` | 本体、手書き | 手書き | 手動**subprocess** |
| Rust | `#[test] #[should_panic(expected = "msg")]` | test属性 | 指定文字列がpanic messageに含まれるsubstring | 対象外、process内unwindのみ。`panic = "abort"`や`process::exit`はharness終了 |
| Swift（XCTest） | `XCTAssertThrowsError(expr)` | 本体 | error値 | 対象外（`fatalError`） |
| Zig | `try std.testing.expectError(error.X, f())` | 本体 | error-set tag | 対象外。panic期待test提案ziglang/zig#1356は**open** |
| pytest | `with pytest.raises(E, match=r"…"):` | 本体context manager | regex、`re.search` | 対象外 |
| Jest | `expect(() => f()).toThrow("msg")` | 本体closure | messageがsubstringを**含む** | 対象外 |
| Kotlin（kotlin.test） | `assertFailsWith<T> { … }` | 本体closure | exception型、返したexceptionを後で検査 | 対象外 |
| JUnit | 4: `@Test(expected = X.class)`; 5: `assertThrows(X.class, () -> …)` | 4 test、5 本体closure | 型 | 対象外 |
| Elixir ExUnit | `assert_raise ArgumentError, "msg", fn -> … end` | 本体closure | Regexまたは完全一致String | `catch_exit` |
| Gleam（gleeunit） | なし、`should.be_error`は`Result`対象、`panic`はtest失敗 | — | — | — |
| Roc | なし、`expect`のみ、`crash`はtest失敗 | — | — | — |

調査結果:

1. **Closureを取るassertが主流。** Jest、Kotlin、Elixir、JUnit5、Swiftが採用し、pytestは同発想のcontext manager。JUnit4のtest属性`@Test(expected=)`をJUnit5は`assertThrows`へ、NUnit3も`[ExpectedException]`を同様に廃止。test全体宣言ではどのstatementが失敗すべきか示せず、setupがthrowしてもpassする。closureが対象statementを特定する。
2. **SwiftとGo以外はclosureをprocess内で捕捉。** Almideにはどのlegにもprocess内catchがない（背景）。
3. **Swift exit testがAlmideに適合する先例。** closure形式を保ち、子processでclosureだけ実行、別processなのでcapture禁止。Goの`BE_CRASHER`は同機構を手書きしたもの。

<a id="decision"></a>

## 決定

<a id="d1-the-surface-testingassert_abortsexpected-string-f----t---unit"></a>

### D1. 表面: `testing.assert_aborts(expected: String, f: () -> T) -> Unit`

```almide
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "reading past the end aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  testing.assert_aborts("expected: none", () => testing.assert_none(some(1)))
  assert_eq(nth([1, 2, 3], 0), 1)   // the test goes on after each assert_aborts
}
```

- **引数**: Q15どおり期待substringが先、closureが最後。Elixir `assert_raise(E, msg, fn)`とKotlin末尾`block`の順。
- **型**: `[T] (String, () -> T) -> Unit`。closure値は捨てるので、`println`で囲まず`() => xs[5]`が型検査を通る。
- **Effect**: effect closure可（透明fn型、ADR-0026）。子でfile/envを読め、作業directoryと環境はtestと同じ。
- **Import**: `testing`は既に`import testing`必須で、`assert_aborts`を他の`testing.assert_*`と並べる。
- **書きやすさ**: モデルが5言語で既に「失敗すべき」に使う形。Almide独自のcapture禁止はruntime失敗でなくcheck時にfix hint付きで強制（D2）。

<a id="d2-check-time-rules-one-new-check-two-diagnostics"></a>

### D2. Check時規則（新check1つ、診断2つ）

すべてcheckerに置く。`crates/almide-frontend/src/concurrent_reach.rs`と`check/concurrent_reach_check.rs`で既にE095（ADR-0020 §5.2: `http.serve`のappは別instance実行のためlocal capture不可）を強制するclosure-reach機構を再利用し、`testing.assert_aborts`の第2引数に新slot kindを与える。

1. **Closureは何もcaptureしない。** `insp-all`で次の空きE098。IDは共有資源なので実装時に再確認。message: `the closure passed to testing.assert_aborts captures \`xs\`, a local of the test`。
   - lambda literal必須。testのlocal、parameter、`where` binding、外lambda parameterはcapture。
   - top-level `fn`、`let`、import module、top-level `var`は参照可。子は新programなので`var`は初期値（D3）、test runnerのtestごとre-initと同じ状態。
   - lambda literalでないfn値（local `f`を使う`testing.assert_aborts("x", f)`）もcaptureを見られずE098。
   - hint: `the closure runs in a separate process (native) or instance (wasm), where the test's locals do not exist. Inline the value into the closure, or make it a top-level \`let\`.`。`try:`はcapture localのbindingがliteralならclosure内へ移した形を示す。
2. **呼出し場所。** 次の空きE099（`insp-all`）。direct callで、lexicalに`test` block内にある必要がある。内部の`if`、`match`、`for`、lambdaへの入れ子は可。以下はerror:
   - `fn`（testから呼ぶhelperも含む）または`main`内。hint: `assert_aborts runs only in a test: move the call into a \`test "…" { }\` block`。abortしたいprogramは単にabortする。
   - first-class使用（`let check = testing.assert_aborts`）。
   - 他の`assert_aborts` closure内の`assert_aborts`。孫が必要になる。

<a id="d3-execution-the-runner-runs-only-the-closure-in-a-child"></a>

### D3. 実行: Runnerは子でClosureだけを実行

**Lowering（frontend、全leg共通で一度）。**
- 各call siteのcaptureなしclosureをtop-level fn `__almd_abort_site_<k>`へlift。`k`はprogram内宣言順site indexで、同buildの親子間で安定。
- callは`testing.__abort_site_run(k)`へ。legごとのintrinsicで、子exit statusとstderrの`(Int, String)`を返す。
- pass/fail比較（D4）は`stdlib/testing.almd`の通常Almide codeとして続け、一度書いて全legへ継承。
- liftしたsite fnはtestでなく**program本体**としてlower:
  - 中の`assert*` / `testing.assert_*`はT18 block形（C-153 / C-381）。
  - nativeでも`panic`はC-219 `PANIC: <msg>`経路。
  これで全legのabort textが同じbyteになる（D4）。

**Native**（test binaryはlibtest、`src/cli/commands.rs::cmd_test`、`runtime/rs/src/testing.rs`）:
- codegenがhidden `#[test] #[ignore = "almide:abort-site"] fn __almd_abort_site_entry()`を1つ生成。`ALMIDE_ABORT_SITE=<k>`を読んでsite `k`を呼ぶ。
- `__abort_site_run(k)`が`std::env::current_exe()`を`--ignored --exact <path>::__almd_abort_site_entry --nocapture --test-threads=1`付きでspawn、`ALMIDE_ABORT_SITE=k`と`ALMIDE_TEST_ABORT_PROBE=1`を設定。待って`(status, stderr)`を返す。
- `ALMIDE_TEST_ABORT_PROBE=1`は`almide_panic!`を`cfg!(test)` unwindからC-219 abortへ切替。runtime読取りなのでclosureのhelperにも届く。
- Go `BE_CRASHER`を手動でなくruntimeが行う。2回目buildなし、通常runはignored entryをskip。

**Wasm**（`src/cli/test_wasm.rs`、`crates/almide-driver/src/test_runner.rs::synthesize`、`crates/almide-wasm-run`）:
- 合成runner `main`へselectorを追加。argvに`--almide-abort-site=<k>`があればglobal init後site `k`だけを走らせ、それ以外は既存test実行。
- `__abort_site_run(k)`はprivate embedded-host import `almide:test/abort-site`へlower（ADR-0025の`almide:process/spawn`同様）。hostは**同じコンパイル済みmodule**をそのargvで新規instantiateし、終了まで走らせ`(status, stderr)`を親instanceへ返す。親は続行。
- importはtest buildとembedded host限定。既定`WasmTestRunner`がこれ。
- `ALMIDE_TEST_WASM_RUNNER=wasmtime`（stock runtime）では`assert_aborts`使用fileは明示wallとしてnative fallback、理由を表示。黙ったdeclineではない。
- `host_ops.rs:243` decline行は`assert_throws`とともに削除。

**Interp**（3-way oracle）: 新interpreter状態でlift site fnを実行しabort text報告。現状native abortで棄権する箇所（`docs/specs/als/expressions.md`裁定3）はここでも棄権。

**1 test複数call。** 各callが順番に別childを走らせ、その都度親test続行。旧polarity testの2call同様、複数abort検査可。`assert_aborts`失敗は他の`testing.assert_*`同様harness報告形式でそのtestを失敗させる。他のfile内testは通常どおり続く。

**子が見るもの。** top-level let/var初期値の新program、testと同じcwd/env/argv、親状態なし。stdout/stderrはcaptureして転送せず、失敗報告（D5）にだけ出る。

**費用。** callごとnative1 process spawn、wasm1回の既コンパイルmodule instantiate。移行後specには4call（D7）。これは見積りで、実装PRが`almide test spec/stdlib/` wall timeを前後測定する。

<a id="d4-what-counts-as-an-abort-and-what-is-matched"></a>

### D4. Abortの定義と照合対象

`testing.assert_aborts(s, f)`は子が*定義済みabort*で終わり、その*abort text*に`s`を含む場合のみpass（大小区別byte substring）。

- **定義済みabort**: exit status **1**、stderrに`Error: `または`PANIC: `で始まるheader行。契約の全abortはこの形:
  - `PANIC: <msg>`（C-219）。
  - `Error: index out of bounds`（C-067）。
  - `Error: division by zero` / `Error: integer overflow`と残りALS-T6。
  - `Error: stack overflow`（C-196）。
  - `Error: exit code must be in 0..=255`（C-350）と他ALS-R1 domain error。
  - `Error: assertion failed` block（C-153 / C-381 / C-339）。
- **Abort text**: stderrの**最後の**header行から末尾。closureの前方`eprintln`は照合を満たせない。
- **Target非依存**: 契約がnative/wasmのbyteを同一固定し、D3はprogram modeで実行するので判定はtargetに依存しない。
- **Abortでないもの**（assert失敗）:
  - closure return（exit0）。
  - 他exit status: `process.exit(n)`の`n ≠ 1`、raw Rust panic（101、C-153級欠陥）、wasm trap（134、未定義abortゆえI-divergence）、signal。
  - headerなしexit1。
- **残る偽陽性1つ**: closureが自分で`Error: …`を出し`process.exit(1)`する意図的模倣。T18 desugar自体`eprintln`＋`process.exit(1)`なので除外にはdesugarも迂回するabort side channelが必要。割に合わない。

<a id="d5-the-failure-report-an-expectedfound-block-like-t18"></a>

### D5. 失敗報告: T18同様Expected/Found Block

`stdlib/testing.almd`の比較失敗はC-153 field順:

```
Error: assertion failed
  at: line 7
  expected: abort containing "index out of bounds"
  found: returned normally
```

`found:`は以下の一つ:
- `returned normally`
- `abort "<abort text>"`（abortしたがtext不一致）
- `exit code <n>`
- `exit code 101 (raw panic)`
- `trap (exit 134)`

子が何か出せば後に`child stdout:` / `child stderr:` sectionを付ける。

test block内は他の`testing.assert_*`と同じharness報告形式。`at:`は`assert_aborts` call行。

<a id="d6-removal-testingassert_throws-leaves-at-dialect-epoch-14-with-no-deprecation-window-ruled"></a>

### D6. 削除: `testing.assert_throws`をEpoch14で非推奨期間なく廃止（裁定済）

`remove-33`に追加するepoch14 `breaks` entry:

```
"`testing.assert_throws(f, expected)` → rewrite as `testing.assert_aborts(expected, f)`: the expected text comes first, and `f` must be a lambda that captures nothing (it runs in a child process or instance); every abort counts — `panic`, index out of bounds, division by zero, a failed assert — not only `panic`. Outside a test block there is nothing to rewrite to: the call could only fail, so delete it",
```

**診断。** `crates/almide-frontend/src/removed_spellings.rs`（`remove-33`）は`breaks`からrepairを読むが、`parse_break`は2形、`→ \`import x\` + \`x.f(…)\` (package …)`と`→ delete the call`のみ認識。そのままでは上entryはprose扱い、callはrepairなしunknown-function E002になる。

実装は第3固定形`→ rewrite as \`m.g(…)\` …`を加え、`Fate::Rewritten { to, repair }`へparseし、同PRで`parse_break` test拡張。callには次を出す:

```
error[E002]: `testing.assert_throws` was removed at dialect epoch 14
  hint: rewrite as testing.assert_aborts(expected, f) — the expected text first;
        f runs in a child and must capture nothing; every abort counts, not only panic
  try: testing.assert_aborts("oh no", () => panic("oh no"))
```

`try:`はcall自身の引数を交換して作る。test block外ではrepairの「削除」側を示す。

<a id="d7-migration-of-the-callers-almide-and-als-the-same-text"></a>

### D7. 呼出し元移行（AlmideとAlsで同じテキスト）

`spec/stdlib/coverage_assert_throws_test.almd`は`spec/stdlib/coverage_assert_aborts_test.almd`へ:

```almide
import testing

test "panic is an abort" {
  testing.assert_aborts("oh no", () => panic("oh no"))
}

test "the expected text is a substring" {
  testing.assert_aborts("went wrong", () => panic("something went wrong"))
}

test "an index past the end is an abort" {
  testing.assert_aborts("index out of bounds", () => [1][3])
}

test "division by zero is an abort" {
  testing.assert_aborts("division by zero", () => 10 / (int.parse("0") ?? 1))
}
```

`spec/stdlib/testing_polarity_test.almd`は2callの1testを維持。D3では両方T18 block textに照合:

```almide
test "wrong polarity fails" {
  testing.assert_aborts("expected: none", () => testing.assert_none(some(1)))
  testing.assert_aborts("expected: err(_)", () => testing.assert_err((ok(0): Result[Int, String])))
}
```

両fileの`// wasm:skip`を削除。`proofs/wasm-fallback-baseline.txt`の`HARNESS_LIMITATION`2行削除（ratchet2減）、`tests/wasm_skip_ledger_test.rs`の`NoUnwinding` entry削除。

`spec/wasm_cross/testing_assert_abort_throws.almd`と6 fixture台帳（run/check/ast/alloc/size/gauntlet）の対応行を削除。

新stdlib fn `testing.assert_aborts`は同PRで通常手順:
- `stdlib/testing.almd`。
- `self_host_registry.rs`（比較）とleg別2intrinsic。
- `docs/stdlib/testing.md`、`almide check`例付き。
- `docs/stdlib/since.toml`。
- `proofs/stdlib-inspection.toml`行、#3671に従いsuccess pathを両leg検査。
- `proofs/target-availability.toml`: native/embedded host対応、stock runtimeはD3のwall。

<a id="d8-contracts-als-first"></a>

### D8. 契約: Als先行

**almide/alsを先に**変更、別commitで`proofs/als-pin.txt`を進め、その後実装PR（als-pin順）。

1. **C-381**（ALS-T18）のstatement編集:
   - assert listから`` or `assert_throws` ``を削除（末尾は`` … `assert_ok`, or `assert_err` ``）。
   - 関係`` `assert_throws(f, s)` -> `panic containing <s>` / found `returned normally` ``を削除。
   - 文`` `assert_throws` outside a test block can only fail: a `panic` there is the C-219 abort, which nothing catches, so the thunk runs once and its panic aborts the program, or it returns and the block follows. ``を削除。
   - 履歴文から`` , or refused `assert_throws` at build time (E081) ``を削除し、次を追加: `` `testing.assert_throws` was removed at dialect epoch 14 (ADR-0029); `testing.assert_aborts` replaces it (ALS-T30), and a call to it outside a test block is a check-time error, so it never reaches this desugar. ``。
   - evidence行`spec/wasm_cross/testing_assert_abort_throws.almd`削除。
2. **ALS-T18**（`docs/specs/als/text-and-numbers.md`）:
   - 箇条書き`` `testing.assert_throws(f, s)` → … ``削除。
   - `` `test` ブロック外の `panic` は C-219 の abort であり ``から始まる段落を削除。末文（関数値として渡した呼び出しは…対象外）は前段末尾へ移して維持。
   - fixture listから`_throws`削除。
   - `docs/contracts/conformance.md`再生成。
3. **新節ALS-T30「testing.assert_aborts — 子で走るabort表明」**（als `origin/main` bc4ab695の次の空きT番号）。D1–D5を規範化:
   - 子process/instanceでclosureだけをfresh program状態で実行。
   - captureとtest外callはcheck error。
   - 定義済みabort、abort text、substring照合。
   - closure内program-mode lowering。
   - 各call後親続行。
   - failure block。
4. **次の空きIDの新契約**（als `origin/main` bc4ab695では382。citation gateがIDと読まぬようprefixなしで記載。ID共有ゆえPR時再確認）。提案statement:

   > ALS-T30: test block内の`testing.assert_aborts(s, f)`は、compile時検査でcaptureなしlambdaとされた`f`だけをfresh子process（native）または同module新instance（wasm）で走らせる。子が定義済みabort（status1、stderrに`Error: ` / `PANIC: ` header）で終わり、最後のheaderからstderr末尾のtextに`s`を含む場合のみpass。nativeの`almide test`と`--target wasm`で同一。全abort対象（`panic`、範囲外index、ゼロ除算、stack overflow、program本体として走るclosureのT18失敗assert）。正常return、他status、raw panic、trapは`expected: abort containing <s>` / `found: …` blockで失敗。各call後testは続行し、複数callはそれぞれ別childを得る。

   証拠（test laneが両legで走らせるfixture class、各`// @contract:` headerに契約名）:
   - `spec/stdlib/coverage_assert_aborts_test.almd`。
   - `spec/stdlib/testing_polarity_test.almd`。
   - `spec/lang/assert_aborts_continues_test.almd`: `assert_aborts`2回後も動作中とassert。

   失敗case（D5の`found:`各形）とE098/E099拒否はRust harness `tests/assert_aborts_runner_test.rs`で両leg固定。
5. **als `ref/src/stdlib_testing.rs:22`**: `"testing.assert_throws"`を`"testing.assert_aborts"`へ置換、参照evaluatorにchild意味論（callごとfresh状態）。
6. **als spec file**: `spec/stdlib/coverage_assert_throws_test.almd`、`spec/stdlib/testing_polarity_test.almd`、`spec/wasm_cross/testing_assert_abort_throws.almd`へD7同編集。

als PRはdialect epochを触らない。epoch14は本repo台帳。

<a id="alternatives-considered"></a>

## 検討した代替案

- **A. Test属性`@aborts("…") test "…" { }`**（初稿、Rust `#[should_panic]`型）。Q15却下、ownerは`@`増加を望まない。test全体対象で失敗statementを特定できず、JUnit5/NUnit3が離れた理由でもある。1test1abort限定。
- **B. Test-header modifier `test "…" aborts("…") { }`。** Q15却下。調査言語になく、同slotの`where_clause*`と競合。
- **C. `assert_throws`のprocess内catchを保ちwasm unwindを実装。** 却下。対象wasm legはunwindなし、nativeも`panic`だけ捕捉。裁定は全leg全abort対象。
- **D. Capture値を子へserializeして許可。** 却下。closure/handle含む全型wire形式が必要で、子入力がsource表示と違う。Swiftも同理由で初版capture禁止。check errorは機械fix可能。
- **E. 比較をAlmideでなくhost/harnessへ。** 却下。`stdlib/testing.almd`なら判定とfailure blockを1desugarで全leg継承（C-153 desugar once）。hostは`(status, stderr)`だけ報告。
- **F. Test外（`main`）call許可。** 却下。programには報告harnessがなく、期待abortはただのabort。wasm importのtest限定も守る。
- **G. 子stderr全体を照合。** 却下。前方`eprintln`の期待文言が満たして、未検査の理由でpassしてしまう。

<a id="consequences"></a>

## 帰結

- **両leg・全abort**を同判定でtest可能、1test複数回。
  - `testing`の`HARNESS_LIMITATION`消滅: native fallback file2減、wasm decline行1減（`host_ops.rs`）。
  - abort closureは残test/fileを道連れにしない。通常testの予期しないabortは引き続きそうなる（`test_output.rs`）、本範囲外。
- callごとspawnまたはinstantiate1回（D3）。
- 新private embedded-host import1つ（`almide:test/abort-site`）、test buildのみ。
- 新check診断2つ（`insp-all`時E098/E099）。capture規則はSwift以外でモデルが見ないLLM向け規則。E098 hintと`try:`が修正を担う。
- `stdlib-inspection.toml`は1行置換、`assert_throws`廃止、`assert_aborts`追加で両leg stamp必要。

<a id="falsifier"></a>

## 反証条件

- owner実行Dojo測定（CIにAPI keyなし）で「関数がXでabortするとtest」の初回正解率。初回多数がlocal captureし、E098修正でも2回目が同等に達しなければD2再考。次候補はliteral scalar限定capture serialize（案D）。
- `assert_aborts` closure内の定義済みabortがleg間でtext/status違いならD4非依存が崩れる。比較へpatchせず、abort契約のI-divergenceとして起票。
- callごと実測費用（D3）が`almide test spec/`をnoise超で遅くするならnativeはsiteをbatch化。

<a id="open"></a>

## 未決項目

2026-10-18に全○（各問いの提案回答）で決定。関係する限りSwift6.2 exit testに従う: effect含む任意codeを環境継承・global初期値の子で実行、宣言exit条件に照合、出力captureは報告用のみ。Q8はstock wasmtime guestにfresh instance開始手段がないため○、embedded hostがwasmを提供しstock laneはwall明示。既裁定は綴り/引数順（Q15）、子process/new instance、captureなし、全abort、epoch14即廃止とtest外削除。`removed_spellings.rs`第3形はD6の実装選択。

1. closure型はUnit限定でなく、値を捨てる`() -> T`（対`() -> Unit`）か。
2. effect closure可か（子fs/env/argv、testと同cwd/env/argv）。
3. top-level `var`を参照して子で初期値を見ること可か。
4. abortはexit1＋`Error: ` / `PANIC: `行、最後のheaderから照合し、`process.exit(n≠1)`、raw panic、trapは失敗か。
5. closureはprogram本体として走り、assertはT18、native `panic`は`PANIC: …`を出すか。
6. test外call（`fn`、test呼出しhelper、`main`）はcheck errorか。
7. first-class使用とclosure内入れ子`assert_aborts`はcheck errorか。
8. stock-wasmtime runnerでは使用fileはwall明示でnative fallback、private `almide:test/abort-site`はembedded hostのみか。
9. 子stdout/stderrはfailure reportのみ、pass時転送しないか。

<a id="references"></a>

## 参考資料

- Swift Testing ST-0008、exit test（Swift6.2で実装）: https://github.com/swiftlang/swift-evolution/blob/main/proposals/testing/0008-exit-tests.md （`expression`は親processで呼ばない。親や外側lexical contextの状態をcapture不可）
- Go、subprocess test (`BE_CRASHER`, `exec.Command(os.Args[0], "-test.run=TestCrasher")`): https://go.dev/talks/2014/testing.slide#23
- Rust Reference、`should_panic`属性: https://doc.rust-lang.org/reference/attributes/testing.html#the-should_panic-attribute （passには指定文字列がpanic messageに含まれる必要）
- XCTest `XCTAssertThrowsError`: https://developer.apple.com/documentation/xctest/xctassertthrowserror(_:_:file:line:_:)
- Zig、panicを期待するtestの提案（open）: https://github.com/ziglang/zig/issues/1356
- pytest `raises(match=…)`, `re.search()`で照合: https://docs.pytest.org/en/stable/reference/reference.html#pytest-raises
- Jest `.toThrow(string)`, messageがsubstringを**含む**、codeを関数で囲む必要: https://jestjs.io/docs/expect#tothrowerror
- kotlin.test `assertFailsWith(message, block)`: https://kotlinlang.org/api/core/kotlin-test/kotlin.test/assert-fails-with.html
- JUnit5のexception test (`assertThrows`): https://junit.org/junit5/docs/current/user-guide/#writing-tests-assertions
- ExUnit `assert_raise/3`, messageはRegexまたは完全一致String: https://hexdocs.pm/ex_unit/ExUnit.Assertions.html#assert_raise/3
- gleeunit `should`: https://hexdocs.pm/gleeunit/gleeunit/should.html
- Rocのtestと期待: https://www.roc-lang.org/tutorial#tests-and-expectations
