<!-- description: Derive call-result layout from the type, not from module/func name tables -->
<a id="result-family-from-type--layout-is-a-function-of-the-type-names-only-choose-code"></a>

# result-family-from-type — layoutは型の関数であり、名前はコードだけを選ぶ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](result-family-from-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](result-family-from-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](result-family-from-type.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/result-family-from-type.md)
> 履歴上のactive roadmapであり、Phase 0–2着地とPhase 3部分実装、未完のregistry導出・Stage Cを区別する。原文は8コンパイラ／8/8と述べるが列挙は7項目であり、この差とrepo外調査の所在を保留し独立に解決しない。構造的に表現不能との主張は記述されたfamily/routingバグ分類についてであり、全コンパイラ障害の保証ではない。

**状態**：active（phase 0は2026-08-14に着地。この文書はこの取り組みの基本規約）
**契機**：#1406と文書化された4つの同類事例 — 1つのバグ分類、5つの事案。
**調査根拠**：`../almide-references/RESEARCH.md`（8コンパイラの調査、file:line引用。repo外に保管）。

<a id="the-disease"></a>

## 病状

MIR loweringは、呼び出し結果の物理layout（「family」）を
`(module, func)`の名前表から決めている：

- `is_self_host_result_module_fn`（scalarのlen-as-tag family、約27行）
- `is_self_host_result_str_module_fn`（heap-Okのcap-as-tag family、約37行）

同じ分類の5事案はすべてtree内に記録されている：

1. **#1406-D7** — 分類箇所が見るのはrouting前の名前（`fan.any_map`は9組を包含し、emit時に`fan_any_call_name`がsuffixを付ける）。そのためrouting後の形式を対象とする名前行は発火せず、String出力の組が誤ったfamilyに分類されてwallになる。
2. **C-145** — mono suffix付きの`or_else__…`は、`base_stdlib_fn_name`ができるまで名前をキーとするすべての判断から漏れた。
3. **#1144** — `__`で始まるcarrier名が空文字列に分割された。名前をキーとする判断はすべて黙って漏れ、fn全体がwallになった。
4. **fs.copy/appendのfamily誤分類** — 一時期str familyに載っていた。すべてのerrがokとして読み戻された（無言の誤分岐。mod_p5.rs:34-49に記録）。
5. **恒常的な点ごとの保守** — stdlibに組を加えるたびに表の行が必要。この表の存在自体が、CLAUDE.mdの「点ごとではなくmatrixで拡張する」という規則に構造的に反する。

<a id="the-root-cause"></a>

## 根本原因

layoutが型の関数になっていない。1つの型`Result[Unit, String]`に、当時は2つの物理layoutがある：

- **len-as-tag**（@4 = 0/1、payload @12）：`materialize_result_ok`経由のctor構築 — fs.copy、fs.append、fs.remove、fs.write_bytes、fs.write_bytes_raw。
- **cap-as-tag**（tag @16、payload @12、Okはlen@4=0という規約）：prim-render構築（$write_text_file family）— fs.write、fs.mkdir_p、fs.rename、fs.remove_all。

分岐がproducerの同一性に依存するため、名前でしか取り戻せない。これが名前表の存在理由である。他のResult型はすでに一貫している：scalar-Ok（`Int`/`Float`）は常にlen-as-tag、heap-Ok（String/List/Value/…）は常にcap-as-tag。

<a id="what-the-surveyed-compilers-do-one-line-each"></a>

## 調査したコンパイラの方式（各1行）

- rustc：return ABI = `arg_of(sig.output())`。intrinsicの名前表（436 arms）は命令を選び、型から導いたlayoutに対してdebug assertする。
- Swift：型キーごとにキャッシュした`TypeLowering*`を1つ持つ（decl/nameフィールドなし）。唯一の名前由来規則（ObjC selector families）はimport時に関数型へ折り込む。
- Zig：`firstParamSRet(cc, return_type, target)`。compiler_rt名は型から構築する。名前は型判断の出力であり、決して入力ではない。
- Roc：各LIR localにintern済みlayoutを格納する。builtin名registryが持つのはsymbolとRC契約であり、layoutは明示的に持たない。
- Lean 4：型→閉じた14variantのIRType。唯一の名前キー結果例外（3つのArray fn）が、強制されない「Keep in sync」コメント付きで3passに重複する — N=3で同じ病状。AlmideはN=64。
- Koka：`cType :: Type -> CType`。唯一の名前表は引数borrowを扱い、結果は常にowned +1（ABI定数なので追跡不要）。
- Grain：一様なtag付きwordと1bitのManaged flag。解放時はruntime tagを読む。

**法則（8/8）：名前はコードを選び、型はlayoutを選ぶ。**

<a id="the-cure-in-phases"></a>

## 段階的な治療

<a id="phase-0--tactical-1406-landed-with-this-doc"></a>

### Phase 0 — 戦術的な#1406（この文書とともに着地済み）
`returns_foreign_result`のcan-err seed、分類箇所での`is_fan_any_map`+`is_heap_ok_result`による型分岐、C-004 fixture。最も問題の大きい対象で型分岐の形を確かめる。この特例はPhase 2で一般則へ解消する。

<a id="phase-1--make-family--fty-true-unify-resultunit-string"></a>

### Phase 1 — `family = f(Ty)`を真にする：`Result[Unit, String]`を統一
`Result[Unit, String]`のok(())/err(m) ctor経路をcap-as-tag materializationへ流す（fs.writeのprim familyがすでに使うlayout：Ok = len@4=0、@12=0、tag@16=0、Err = len@4=1、@12=msg、tag@16=1）。ctor構築の5つの名前をscalar表からstr表へ移す。drop routingは変更しない（このlayoutの両armに対して平坦な`DropListStr`が正確である — fs.writeの先例）。この段階の後、型→family写像は全域となる：

```
result_family(Result[T, E]) = HeapOk (cap-as-tag @16)  if T is heap or Unit
                              Scalar (len-as-tag @4)   otherwise (Int/Float/Bool)
```

gate：全corpus（418のwasm_cross fixture、3-way）、ABI probe差分、suite。

<a id="phase-2--one-family-function-name-tables-carry-no-family"></a>

### Phase 2 — family関数を1つにし、名前表にはfamilyを持たせない
`is_heap_ok_result`の隣に`result_family(ty: &Ty) -> ResultFamily`を唯一の判断点として導入する。2つの名前表を「この呼び出しの結果はmaterializeされている」だけを意味する1集合へ統合する（両表の和集合）。分類箇所（bind：binds_p2_c.rs、subject：tracked_calls.rs + control.rs、control_p2_b.rs:30/73のheap-ok refinementのやり取り）を`materialized(name) && family(ty)`へ書き換える。`is_fan_any_map`は削除し、一般則で扱う。fs.copyの事案は表現不能になる：行がfamilyを持たなくなるため、型を誤ったfamilyに置く行を作れない。

<a id="phase-3--derive-materialized-from-the-registry-kill-the-hand-table"></a>

### Phase 3 — registryから「materialized」を導く（手書き表を廃止）
残る名前集合は、self_host_registryとprim floorがすでに持つ知識の重複である。それを導出する：Module呼び出しがResultをmaterializeするのは、routing済み名（唯一のrouterである`list_heap_call_name`）が`self_host_registry`でlinkされる場合（ctor経路のlayoutは構築上canonical）、またはprim floorの場合に限る。registryでlinkされたResult返却fnすべてを走査し、分類箇所と`result_family`の一致をassertする実行可能gateを着地させる — matrix-gateの規律、gateされたmatrixはdriftできない。

<a id="phase-4--seed-once-stage-c-direction--close"></a>

### Phase 4 — 1回だけseedする（Stage-Cの方向）＋完了
値ごとのrepr設計（Rocの`Local { layout_idx }` — 「未追跡」を表現不能にする）と、その第一歩として1回だけseedする入口は#1414として起票済み。これは自然な後続であり、この取り組み自体には含めない。統一する集合挿入の仕組みはdrop-routingコードで、挙動を変えないrefactorでも実際のリスクがあるためだ。一方、この取り組みが集中化を目指した判断ロジック（`result_family`＋統合集合）はすでに単一点になっている。ARCHITECTURE.mdを更新し、計測差分とともにこの文書をdone/へ移す。

<a id="landed-state-2026-08-14"></a>

## 着地状態（2026-08-14）

- Phase 0：cd0d01820..f837fa0a5（can-err seed、型分岐、C-004 fixture、この文書）。
- Phase 1：`materialize_result_err_str`（ctor Result-err blockの@16 tag — 上位互換でlen readerは不変）、ctor構築の`Result[Unit, String]`の5fnをstr familyへ移した。`is_result_unit_str_ty`を`??` scalar経路で許可した。`Result[Unit, String]`は1つのlayoutになった。
- Phase 2：`result_family(ty)`＋`is_self_host_materialized_result_fn`（統合集合）。tracked_calls.rs / binds_p2_c.rs / control_p2_b.rsが参照する。#1406の`is_fan_any_map`特例とcontrol_p2_bのheap-ok回避経路は一般則へ解消した。
- Phase 3（最初の部分）：crates/almide-mir/src/lower/tests_result_family.rsの3gate testがfamily関数の全域性、9組1名の分岐、名前manglingの事案分類（C-145、#1144）を固定する。統合集合の完全なregistry導出はこの取り組みの未完部分 — registryに宣言戻り型の列が加わった時に行う。それまでは統合集合はfamilyの意味を持たない2つの格納表である。
- Stage C：#1414。
- `??`経路の乱立（1段上の同じ病状 — 点ごとの許可リストの背後にpayload分類別の5経路）：2026-08-15に封じ込め。末端scalar経路は型guardするようになり（許可されたがroutingできないoperandは正直なwallへ降り、無言の誤分岐という分類は表現不能）、許可リストは名前付きの単一情報源述語（`unwrap_or_heap_ok_route_exists` — familyではなく経路の能力）となった。最終形のdesugar（`?? → match`、1回の書換えで乱立を削除）は`ALMIDE_QQ_DESUGAR=all`の背後に実装され、計測残数は13fixture — 受入matrixは#1418にある。途中で2つのcoverage欠落も閉じた：`?? ()`（Unit-is-0 fallback arm）とユーザーfnの`Result[Unit, String] ??`。

<a id="option-family--surveyed-2026-08-14-no-family-disease"></a>

## Option family — 2026-08-14調査、familyの病状なし

`is_self_host_option_module_fn`（mod_p4.rs）は同じバグ分類について監査し、問題なし。名前表は「materialized」だけを意味する（Result表がPhase 2でなった形と同じ）。payload routingは分類箇所ですでに型駆動（`is_heap_elem_list_ty`、`opt_str_int` tuple経路など）であり、checked数値変換familyは形によって許可しdrift gate（`checked_conversion_family_is_admitted`）を持つので、Resultsの以前の状態より進んでいる。共通する負債は下記のregistry導出の残りだけである。

<a id="non-goals"></a>

## 非目標

- len-as-tagとcap-as-tagを物理的に1block形状へ統一すること（Phase 1は1つの衝突だけを除去。scalar-Okはlen-as-tagのままで、この分岐は型から計算できるため問題ない）。
- Option / Result以外のvariant（同じpatternだが、この後の別の取り組み）。
- v0側の変更（v0は独自loweringを使い、3-way oracleが守る）。
