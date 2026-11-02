<a id="open-issue-inventory-2026-09-21--78-issues-under-one-header"></a>

# 未解決issue一覧、2026-09-21 — 共通headerを付けた78件

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](issue-inventory-2026-09-21.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](issue-inventory-2026-09-21.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](issue-inventory-2026-09-21.zh-CN.md)

> 非規範的な固定履歴参照版。原本は不変の正本で、自動同期しない。引用コマンド・CI変更・測定・現行issue再調査は実施していない。issue一覧の件数と日付時点の状態は、重複除去4件とscript7件/#2400説明、判断待ちと1PR修正分類の不一致も含め原文どおり保持する。現在の作業一覧ではない。Web APIとdeployment adapterは提案であり、前提機能が出荷済みでもframework出荷済みとはいえない。比較や260KBは過去の原文の主張で、独立再測定していない。コードと旧API表記は保持するが、実行可能と認証していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21.md)

v0.63.0 final tagの前夜、すべてのopen issueに共通headerを付けた後のsnapshot（`A-*` crate label一つ、閉集合の状態、1行要約と根拠。軸とheaderは#2417の`docs/project/ISSUE-TAXONOMY.md`、7agentが用いた記入仕様は`issue-inventory-2026-09-21/issue-header-spec.md`）。数値はその時点のopen issue78件に対するcounter自身の自己検査であり、手集計ではない。`issue-inventory-2026-09-21/render-issue-index.py`が`issue-inventory-2026-09-21/issue-index-2026-09-21.txt`を生成した。数が動いたときはこのページを編集せず再実行する。

```
open issues: 78   with header: 78   without: 0
  no A-* label:                                0
  state outside the closed set:                0
  state=BLOCKER vs I-*/regression mismatch:    0
  two A-* labels: 7 (#1423 #1479 #1480 #2141 #2143 #2289 #2387 — each really spans two crates)
```

途中で計測器のbugを一つ踏んだため、後で誤読を引用しないよう記録する。状態名`計測器・ゲート`は区切り`・`を含むので、最初の`・`で分割すると切れてしまい、counterが**「閉集合外25件」**と報告した。データではなくparserの問題だった。現在の記録では既知集合を長い順にprefix照合する。

<a id="release-blockers-4"></a>

## Release blocker（4件）

`count-release-blockers.sh`がfinal tagのgateとする集合。scriptはlabelごとの件数を足し、#2400にblocker labelが二つあるため**7**と出す。重複を除くと4件。

| issue | crate | 2026-09-21の状態 |
|---|---|---|
| #2395 | A-wasm | PR #2399進行中 |
| #2397 | A-wasm | PR #2409、draft。#2404待ち（同じledger行） |
| #2398 | A-wasm | PR #2404進行中。#2399着地後rebase |
| #2419 | A-wasm | branchで修正済み。judge側almide/als#87は2026-09-21 merge、PR #2420 |

着地順：#2399 → #2404 → #2409。#2420は三つと独立。

<a id="by-state"></a>

## 状態別

| 状態 | 件数 | 注記 |
|---|---|---|
| BLOCKER | 4 | 上記 |
| 実装 | 32 | 前提待ち3（#1005→#1003、#1584→#1696、#2147→#2149）、判断待ち3（#1107、#1696、#2010）。**着手可能26** |
| 計測器／gate | 25 | 判断待ち3（#924、#1963、#2382）、前提待ち1（#2009→#1997）。**着手可能21** |
| 設計判断 | 7 | #1460 #1479 #1480 #1490 #1679 #2143 #2162 — すべて○×が必要 |
| 追跡 | 5 | #586 #1514 #1628 #1998 #2302 |
| 調査 | 5 | #2078 #2099 #2150 #2156 #2415 — 最初のstepは計測 |

<a id="by-crate"></a>

## Crate別

| label | 件数 | issue |
|---|---|---|
| A-ci | 25 | #586 #924 #1423 #1469 #1508 #1514 #1617 #1998 #2009 #2141 #2143 #2146 #2152 #2163 #2380 #2381 #2382 #2387 #2390 #2391 #2400 #2402 #2403 #2405 #2406 |
| A-wasm | 22 | #1315 #1584 #1628 #1696 #1710 #1997 #2010 #2099 #2141 #2143 #2150 #2156 #2312 #2318 #2387 #2395 #2397 #2398 #2407 #2408 #2411 #2419 |
| A-codegen | 9 | #1331 #1480 #2044 #2045 #2162 #2289 #2293 #2393 #2410 |
| A-driver | 8 | #1003 #1490 #2092 #2147 #2148 #2151 #2153 #2372 |
| A-perf | 6 | #1963 #2142 #2157 #2302 #2331 #2415 |
| A-frontend | 5 | #1388 #1456 #1479 #1589 #2149 |
| A-stdlib | 5 | #1107 #1423 #1460 #2078 #2289 |
| A-ir | 3 | #1005 #1479 #1480 |
| A-runtime | 1 | #1679 |
| A-modules | 1 | #2332 |

A-ci 25 + A-perf 6 = 31件が計測器とgate。言語・コンパイラ本体（wasm 22 + codegen 9 + frontend 5 + ir 3 + stdlib 5）は44件。同程度の規模であり、言語機能より品質gateを優先するという方針がbacklogの形に表れている。

<a id="the-a-ci-25-sorted-by-what-closing-them-takes"></a>

## A-ci 25件を完了条件で分類

この節の問いはCI枠を一掃できるか。14件は可能。残りは継続的取り組みか、先に判断が要る。

**設計上の未決事項のない1PR gate修正（14件）。** 各々script／workflow変更であり、前後を計測できる。改善CI（#2386、#2416、#2418）がdevelopに入れば、issueごとのPRで並行実施できる。

| issue | 修正内容 |
|---|---|
| #2400 | label別合計でなく重複除去したissueを数える |
| #2403 | `check-als-pin.sh`でidに加えstatementも比較 |
| #2406 | 全`C-NNN`引用をledgerで検証し、stdlibのdangling引用1件を修正 |
| #2405 | manifest generatorが古いtreeを拒否し、gateが現在飛ばすfieldを読む |
| #2402 | domain-edge matrixでjson signature22件すべてを覆う |
| #2382 | fuzz ladderで両legのresource-limit abortをsemanticでなくresourceへ分類 |
| #2390 | nightly fuzz reportで中断shardを0でなくunknownにし、本文20項目上限を撤廃 |
| #2391 | ledger-counts修復をPRとして届け、nightlyに修正branchを明記 |
| #2387 | domain-edges i32_max行を空きmemoryでなくcompilerで判定（決定的上限または明示skip） |
| #2143 | wasm runtime ratchetを`GITHUB_ACTIONS`下で判定 |
| #2141 | stdlibをlinkするcorpusでwasm size ratchet |
| #2163 | 最後のalmide-gates twin二つを`.sh`原本とdiffして昇格 |
| #2380 | fold-seed-consumes-its-own-source形のsource-level gate |
| #2381 | critical path 55分 → #2386着地後に計測。残る分割は数値を伴う後続 |

**2026-09-21に決定し、着手可能になった3件。** #924：連続日数は計画fuzz-minutesの75%以上を提供した夜を数える（`shards=k/N`から読む）。`if: always()` verdictでは構造上満たす「full budget」は数えない。連続日数が読む行は過少計数なので#2390を先に着地させる。#1998／#2009：二つの義務を**S2／S3**（contract-relative survival ladder）と命名。`docs/specs/edit-locality.md:27,33`のL2／L3（cross-target agreement、diagnostic locality）は不変。両裁定はissueコメントにある。

**一掃でなく継続的取り組み（8件）。** #586と#1514（追跡issue）、#1508（reference-suite mining）、#1469（doc generator。「generatorなし」の前提は誤りで、stdlib doc45中43に生成indexがある）、#1423（target-availability matrix）、#2152（portable trust／attestation）、#2146（公開MSR harness、Dojoの領域）、#1617（Sonnet 5でDojo再実行。コード変更でなく実行）。

<a id="premises-that-had-moved--recorded-in-the-headers-not-deleted"></a>

## 変わっていた前提 — 削除せずheaderへ記録

20件以上のissueは前提が既に真ではなかった。各発見はagent一人の安価なtree確認（grep、`ls`、行の読取り）で、本文をそのまま残しheaderの1行要約と根拠へ記載した。**どれもissueをcloseしたり範囲変更したりするものではない。それは人の判断のまま。**

- #2044 — item 1は消えた（`pass_auto_parallel.rs`退役）。`fannkuchredux`は既にVICTORYで、`mandelbrot`行だけ残る。
- #2045 — `|>`は既にfuseする。残るのは`list.range`が`Vec<i64>` headを構築する点。
- #2318 — direction 1着地済み（`region.rs`の`forget_window_temps`）。
- #2150 — native `AlmideMap`はlinear scanでない（16entry超ではindex）。`research/benchmark/perf/README.md:95`の「linear-scan」は古い記述で、別PR予定。
- #1584 — structuralは既定。fallback baseline 25 → 8。残りは#1696のincumbent退役待ち。
- #1514 — unchecked childは#924以外すべてCLOSED。
- #1469 — 「generatorなし」は誤り（stdlib doc43/45に生成index）。
- #2152 / #2151 / #2149 / #2142 / #2163 / #1423 / #1710 / #2312 / #2387 / #2391 — 一部着地済み。残りは各headerに明記。
- #1107 — `@deprecated(since=3)`期間はdialect 4で終了。`unwrap_or(`は残47箇所でなく128箇所と悪化。
- #1388 — 移行costは「2 + 37」から71fileへ増加。

実在した設計上の危険は解消済み：#1998と#2009が提案した「L2／L3」は`docs/specs/edit-locality.md`で別意味に使用済みだった。2026-09-21以降はS2／S3（上記）。

<a id="ci-what-the-first-improvement-measured"></a>

## CI：最初の改善の測定結果

同夜にCI変更3件が着地：#2386（shard重み再測定）、#2418（clippy ratchetとrun_parityを持つpre-push hook）、#2416（required集合greenでenqueueし、mutation gateは着地後developを判定）。最初の変更はAPIのjob時刻、`Test Rust (shard N/4)`のwall minutesで前後測定し、結果は否定的：

| run | head | shard 0 | 1 | 2 | 3 | max |
|---|---|---|---|---|---|---|
| push（旧重み） | e39133dc5 | 57.6 | 24.8 | 44.1 | 36.9 | 57.6 |
| push（旧） | 636a9f771 | 57.0 | 41.4 | 46.2 | 36.5 | 57.0 |
| push（旧） | e8c95e285 | 59.0 | 24.1 | 45.8 | 31.6 | 59.0 |
| push（旧） | 824ba091f | 58.3 | 42.4 | 33.0 | 36.6 | 58.3 |
| #2386を含むmerge_group | 281f23135 | 60.0 | 42.0 | 57.4 | 41.1 | 60.0 |
| push、新重み | 281f23135 | 60.0 | 25.6 | 41.1 | 27.9 | 60.0 |

shard 0は変更前57–59分、後は2回とも60.0。重みの効果は実行間noise内だった。機構：shard 0の1時間は1step（`Cargo tests`58.9分、download 0.6分）で、重みはtest target全体をshard間移動するだけ。単独59分stepはどの割当でも短縮できない。手段はtarget分割か最も遅いtargetの作業削減（#2381）で、target一覧やshard数を変えるPRは同時に`scripts/ci-test-weights.txt`を再生成する必要がある。測定上の罠を二つ記録：docs-only pushではshard jobが3–4秒のno-opになるため、前後比較行が実際にtestを走らせたか確認する。また1時間jobのjob別log APIはsetup stepまでで途切れ、step内のtarget別内訳はなお未測定。

最初のpushで測ったpre-push hookのcost：clippy側は固定toolchain外で0.09秒で`NO VERDICT`（このmachineにrustupはなく、CIがratchetを判定）、run_parityはpushごと103–159秒。30–56分のCI一周に比べ割に合い、同夜に無駄になった3周中2周を防げたgateである。注意する形は反復pushで習慣的に`LEFTHOOK=0`を使って飛ばすこと。

<a id="open-follow-ups-from-the-inventory-pass"></a>

## 一覧化から残った後続事項

- #2398の古さ：同夜に公開assetで測定した報告programはv0.62.0で再現しない。v0.62.0..v0.63.0-rc1の`git bisect run`（431commit、10release build）は092c40d05（PR #2048、typed drop glueのMap/Set entry、2026-09-08）を最初のbad commitとし、報告programには`regression`labelを付けた。しかし欠陥自体はもっと古い。literal listの別fixtureは公開0.62.0 assetで解放済みmemoryを表示する（同夜測定。guard無効でも`list.update`なしのheap保持4経路はcleanなので、mapはheap配置を変えており別carrierではない）。092c40d05は欠けたretainをどのprogramが露呈するかを変えたのであって、欠陥の導入時期ではない。step script、reproducer、両run log、判定表は`issue-inventory-2026-09-21/bisect-2398/`。判定表はrun logから再構成したもので、git自身のbisect logではない（それは`git bisect reset`後に残らない）。周囲のallocationに依存するtriggerという限定は残る。主張は「このprogramは0.62.0で露呈しない」であって、「0.62.0が正しい」ではない。
- #2417（header形式とlabel軸）と#2386（shard重み）はsnapshot時点でopen。
- `research/benchmark/perf/README.md:95`のlinear-scan旧記述 — 別docs PR。
