<a id="adr-0031-a-pointer-the-program-can-read-through-carries-its-provenance--rawptr-splits-into-an-opaque-handle-and-a-bounds-checked-region-base-length-owner-and-a-foreign-pointer-becomes-a-region-only-where-its-extern-declaration-states-the-length"></a>

# ADR-0031: 読み取り可能なポインタは由来情報を持つ — `RawPtr`を不透明な`Handle`と境界検査付き`Region`（base・length・owner）へ分け、外部ポインタは`@extern`宣言が長さを示す場所でのみ`Region`となる

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0031-raw-pointers-carry-provenance.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0031-raw-pointers-carry-provenance.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0031-raw-pointers-carry-provenance.zh-CN.md)

> 固定原文の非規範的な歴史読者スナップショットで、新たな安全性認証や現行実装監査ではない。原本不変更・正本、自動同期や独立テスト実行なし。10月27日のABI制限と29日の修訂を別の履歴として保持。component-modelのcopy loweringはここでは未導入と明示される。境界検査は宣言/host申告の長さに依存し、native foreign codeとwasm hostは信頼対象。host上のread-onlyは契約であってcapability強制ではない。C貸与にはruntime escape backstopがあり、wasm inbound貸与にはない。静的E119追加と「checkerに非escape規則なし」という旧調査/代替案記述の併存は原文履歴として保持し、勝手に統一しない。zero-copyやUAF除去は記述された所有経路・信頼前提の範囲で、普遍的な無脆弱性保証ではない。Dと所有者決定の番号差も原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md)

- **状態**: 採択（2026-10-27）。ownerは[所有者の決定](#owner-decisions-)全8項目に○と回答。導入範囲と限定解釈は[実装](#implementation-2026-10-27)を参照。2026-10-29修訂: [host境界のRegion](#amendment-2026-10-29-regions-at-the-host-boundary)。
- **日付**: 2026-10-27
- **範囲**: `RawPtr`型、`ffi` module、ポインタ用`@extern`シグネチャABI（`crates/almide-types/src/types/extern_abi.rs`）、非推奨`bytes` raw-pointer綴り、security表の行5。
- **関連**:
  - `docs/roadmap/active/security-by-construction.md` 行5（FFI/raw pointer経由メモリエラー、状態partly）。
  - #4043（`import ffi`がopt-in、E100）、#4008（`@memory_unsafe`、critical profile、`Built`行）、#3054（E090、`@extern(c)` ABI表）、#3420（アドレスがblockをborrow）、#3966（`bytes.as_mut_ptr`経由の書込みが`let`を変更）。
  - `tests/raw_ptr_surface_test.rs`: 裁定不要で本ADRとともに導入した部分。

<a id="context"></a>

## 背景

行5の残る穴はその記述によれば、`import ffi`を書いたfile内で、アドレスと長さがprogram所有メモリを指すか検査されず、両targetで`RawPtr`が裸のアドレスなので検査もできないこと。

当時の`RawPtr`:

| | native | wasm |
|---|---|---|
| 表現 | `*mut u8` | 線形メモリ内`i32` offset（MIRでは`i64`） |
| 付随する長さ | なし | なし |
| 付随するowner | なし（#3420は値でなくborrow解析でsource blockを生存させる） | 同じ |

ユーザーprogramの取得経路（宣言シグネチャから導出し`tests/raw_ptr_surface_test.rs`で固定）:

1. **所有する`Bytes`のアドレス**: `ffi.address(b)`、非推奨`bytes.as_ptr` / `as_mut_ptr`。生成時にはownerと長さが分かるのに捨てる。
2. **外部関数の戻り値**: `@extern(c, …) fn f(…) -> RawPtr`。割当の記録は外部libraryだけが持つ。

他に生成元はない。ユーザーfileが名指しできる`Int → RawPtr`経路はなく（`prim.int_to_ptr`はE085の裏）、`RawPtr`算術、literal、Codec fieldもない。ただし`ffi.to_int`と`bytes.data_ptr`を通じてアドレスが`Int`としても移動し、wasmの`@extern`が`(ptr: Int, len: Int)`として受け取る（`spec/wasm_host_js/extern_data_ptr_lifetime.almd`）。

長さが失われるのは正確に2箇所で、両方ともcompilerが何か知る場所である。(1)runtimeはblock長を知り、(2)宣言はbinding作者が外部関数の事実を記す唯一の場所。本ADRはその生成点で由来情報を付け、各accessを照合する。

<a id="survey-what-other-languages-put-in-a-pointer-measured-2026-10"></a>

## 調査: 他言語はポインタに何を持たせるか（2026-10測定）

| 言語 | 読取り可能なポインタ | 長さの出所 | Owner / lifetime | 信頼するもの |
|---|---|---|---|---|
| **Rust**（strict provenance、1.84からstable、2025-01） | `&[T]`は(ptr,len); `*const T`は抽象機械でprovenanceを持ち、`addr()` / `with_addr()` / `without_provenance()`が整数とpointerを区別; `with_exposed_provenance`が明示escape | `slice::from_raw_parts(ptr, len)`という`unsafe`呼出し、変換時に一度長さを表明 | `&[T]`はborrow checker、`*const T`にはなし | 各`unsafe` block、Miriのprovenance検査はtestだけ |
| **CHERI**（Arm Morello、CHERIoT） | capability: address + base + bounds + permissions + hardware validity tag | 割当時設定、縮小のみ可（単調） | 整数はcapabilityになれず、上書きでtag消去 | 初期boundsを設定するallocator |
| **Zig** | `[]T` slice(ptr,len)、Debug/ReleaseSafeで境界検査; `[*:0]T` sentinel終端; `[*]T`長さ不明many-pointer; `[*c]T`は翻訳Cのみ | `ptr[0..n]`: slice化時に長さを表明 | なし | slice化箇所、`@ptrFromInt` |
| **Swift** 6.2 | `UnsafeBufferPointer`（base,count、debugのみ検査）; `Span` / `MutableSpan` / `RawSpan`は非escape（`~Escapable`）view、常時境界検査、ownerにlifetimeを結合; SE-0458 `-strict-memory-safety`がunsafe使用を全て標示 | Clang `-fbounds-safety`の`__counted_by(n)` / `__sized_by(n)`付きC宣言を`Span`引数のsafe overloadとしてimport | viewが依存するowner | `__counted_by`注釈 |
| **Go**（cgo） | `unsafe.Pointer`; `unsafe.Slice(ptr, len)`; `C.GoBytes(p, n)`はcopy | `unsafe.Slice` / `C.GoBytes`で表明 | pin（`runtime.Pinner`、1.21）なしではCはreturn後Go pointer保持不可; `GODEBUG=cgocheck=1`が動的検査 | 変換時長さ、C動作 |
| **Wasm component model**（WASI 0.2 / 0.3） | component境界をpointerは越えない | canonical ABIが`list<u8>`をcopy（`cabi_realloc`経由lift/lower）、受信側はruntimeが測った長さを得る | `resource`は`own<T>` / `borrow<T>` handleとして越境、table indexを毎回検査; `borrow`はcallで終了、`own`破棄でdestructor | component自身のcode |

共通点:

- **1種類でなく2種類。** 返送するだけのtoken（CM `resource`、Rust `*mut Opaque`、Swift `OpaquePointer`）と読取るview（slice、`Span`、capability）を分離する。Almideの`RawPtr`は両方だった。
- **長さはpointer入口で一度表明し、その後毎回検査**: `from_raw_parts`、`ptr[0..n]`、`__counted_by`、`unsafe.Slice`。表明箇所が小さく数えられる信頼基盤。
- **整数はpointerではない。** CHERIはhardware、Rust strict provenanceはAPI、CMはpointerを持たないことで強制。
- **ownerはviewの一部**: Swift `Span`はowner依存、CM `own` handleはdestructor、Goはpin。

<a id="decision"></a>

## 決定

<a id="d1-rawptr-splits-into-two-types"></a>

### D1. `RawPtr`を2型に分ける

- **`Handle`**: 不透明な外部token（`FILE*`、`sqlite3*`、context pointer）。`@extern` fnへ渡すだけ。これを通じ読み書きする`ffi`関数はなく、`Int`形もない。当時の`@extern(c) … -> RawPtr`の大半に相当し、CMの`resource`である。
- **`Region`**: 読み書き可能なviewで**(base,length,permission,owner)**を持つ。`ffi.read(r, off, n)`と`ffi.write(r, off, b)`は両targetでメモリに触る前に`0 ≤ off`と`off + n ≤ len`を検査し、read-only regionへの書込みを拒否する。

表現: native `struct { ptr: *mut u8, len: usize, perm: u8, owner: Owner }`、wasmは3つの`i32`とowner slot。`Region`は`Bytes`同様Perceusのheap値なのでowner参照を数える。

<a id="d2-a-region-is-minted-at-exactly-three-places-each-knowing-the-length"></a>

### D2. `Region`生成は長さを知る3箇所だけ

1. `ffi.region(b: Bytes) -> Region`: **read-only**、長さ`bytes.len(b)`、owner=`b`（RC shareを1つ持ち、全viewよりblockが長生きする。#3420のborrow解析でなく所有によるlifetime）。
2. `ffi.alloc(n: Int) -> Region`: **writable**、新規、unique。`ffi.to_bytes(r) -> Bytes`でcopy出力。writable regionはAlmide値をaliasせず、#3966のpointer書込みで`let`変更という形は書けない。
3. 長さを宣言した外部戻り値:

   ```almide
   @extern(c, "z", "zbuf_new")
   @region(len = n, release = "zbuf_free")
   fn zbuf_new(n: Int) -> Region
   ```

   `len`はparameterまたはconstant名。`release`は解放C関数名で、最後の`Region` share破棄時に実行する。`@region`なしの同宣言は`Handle`を返す必要がある（それ以外E090）。Clang `__counted_by` / Swift safe importをbindingへ移したもの。

<a id="d3-a-region-argument-crosses-the-c-abi-as-two-arguments"></a>

### D3. `Region`引数はC ABIで2引数になる

`fn zfill(buf: Region)`はC prototype `void zfill(uint8_t *buf, size_t len)`にbindする。calleeが得るのはcaller手入力でなくruntime保持の長さなので、Cへregionを渡して長さを過大申告できない。wasmでは`Region`は`@extern(wasm)`を`(i32 ptr, i32 len)`で越え、component(p2/p3)ではcanonical ABIで`list<u8>`へlower（copy）、`Handle`は`own<resource>`となる。

<a id="d4-addresses-as-integers-leave"></a>

### D4. 整数アドレスを廃止する

`ffi.to_int`、`bytes.data_ptr`、非推奨`bytes.as_ptr` / `as_mut_ptr` / `from_raw_ptr` / `copy_to_ptr`を削除。`(ptr: Int, len: Int)`を受けたwasm hostは`Region`を受ける（D3）。以後ユーザーが到達できる`Int`とpointer間の変換は両方向ともない。

<a id="d5-landing"></a>

### D5. 導入

dialect epoch1回、非推奨期間なし（不一致形に関するproject裁定）。`almide fix`は`ffi.address(b)`を`ffi.region(b)`へ、長さなし`@extern … -> RawPtr`を`-> Handle`へ書換える。`Built`行は全`@extern` fnを引き続き示し、全`@region`長さ条項も追加する。

<a id="what-stays-trusted-after-d1d5"></a>

### D1–D5後も信頼するもの

1. 各`@region(len = …)`条項: 外部関数がそのbyte数を返すというbinding作者の表明。宣言ごと1つ、`Built`行で数え、`--profile critical`で拒否。
2. 外部code自体: native Cはprocess内のどこでも書ける。process/sandbox分離なしに言語はこれを閉じられない（CHERI hardware、またはhostを外部codeとするwasm）。
3. `ffi.read` / `ffi.write`のruntime本体、`scripts/check-runtime-unsafe.sh`で数える。

行5は自身の定義で閉じられる: 機構は境界検査、gateは生成種別（owned、allocated、foreign）ごとの差分fixtureで、`len`と`-1`のaccessが両targetで`Err`になること、加えて`tests/raw_ptr_surface_test.rs`を`RawPtr`から`Region`へ拡張する。

<a id="implemented-now-needs-no-ruling"></a>

## 当時実装済み（裁定不要）

`tests/raw_ptr_surface_test.rs`はD2前提を固定する: provenanceは生成点だけで付けられるので、生成点を既存2つに限定する。名前listでなく宣言シグネチャから導出:

- シグネチャに`RawPtr`を持つ全ユーザー到達可能stdlib fnは`@memory_unsafe`。
- `RawPtr`を返す全ユーザー到達可能stdlib fnは`Bytes`を受ける。
- `Int`から生成するmodule（`prim`）はE085と同じ宣言（`stdlib::is_user_nameable_module`）で除外し、除外が空虚でないことも検査。
- `import ffi`を書いたfileでも、`RawPtr`算術、`RawPtr`型の整数literal、`RawPtr` Codec field、`prim.int_to_ptr`を、それぞれclean controlとともに拒否。

<a id="implementation-2026-10-27"></a>

## 実装（2026-10-27）

全8決定をdialect epoch 15に導入、非推奨期間なし（`proofs/dialect-epochs.toml`で削除綴りごとに`breaks` entry。`almide fix`は引数保持のcall改名、`ffi.address(b)` / `bytes.as_ptr(b)` / `bytes.data_ptr(b)` → `ffi.region(b)`、型`RawPtr` → `Handle`を行い、残りはhint）。

- **D1。** `Handle`は旧`RawPtr` builtin head。`Region`はstdlib/ffi.almdの`mod type Region = { owner, base, len, writable, keep }`でmodule外不透明（E033）、programは`base`を読めず`len`を偽造できない。3 raw操作`__address`、`__load`、`__store`は同fileの`local`（他からE420）で、`read` / `write`境界検査後のみ走る。native実装はruntime/rs/src/ffi.rs、wasmはstdlib/ffi_bridge.almd。
- **D2。** `ffi.region(b)`（read-only、owner `b`）、`ffi.alloc(n)`（`effect fn`なので2callを1共有bufferに統合しない）、`@region(len = …, release = "…")`宣言付き`@extern(c)` fn。check/extern_abi_check.rsが条項検証し、欠如または`len`が整数parameterを指さなければE090。`Built`行で数える。
- **D3。** `@extern(c)` Region parameterはglueの`(*mut u8, usize)`（walker/extern_c.rs）。`@extern(wasm)` Region parameterはstructural emitterが読む前に`(base, len)`へ分解（crates/almide-wasm/src/extern_region.rs）、JS glueも同じ2数値を見る。
- **D4。** foreign Regionもread-only。「writableは`ffi.alloc(n)`のみ」を字義通り採る。foreign libraryは自身のbufferを書き、programはRegion（D3）を渡して埋めさせる。
- **D5。** `ffi.read` / `ffi.write`はoffset・count・length付き`err`を返し、list indexは引き続きabort（C-067）。
- **D6。** glueはRegionの`keep` closureにrelease guardを保存。全copyは1つの`Rc`を共有し、最後のcopy破棄時にrelease関数を1回実行。
- **D7。** `ffi.to_int`、`ffi.address`、`bytes.data_ptr`と他4つの`bytes` raw-pointer綴りを削除。`prim.int_to_ptr` / `ptr_to_int`はstdlib自身の下層として残る（外はE085）。

限定解釈し、ownerが別を望むなら裁定事項として明記:

- D3のwasm`(i32 ptr, i32 len)`: emitter整数scalar（importでは`i64`、JS hookではnumber）として越境し、他の`Int` import parameterと同じABI。`i32`へ狭めるにはwasm import ABIの新scalar classが必要。
- D3のcomponent model lowering（`list<u8>` / `own<resource>`）は本変更に含めない。当時p2/p3 componentは`@extern(wasm)` hookを受けない。

Gate: tests/raw_ptr_surface_test.rs（シグネチャ導出: ユーザー到達stdlib fnはHandleを持たず、全Region fnが`@memory_unsafe`、偽造綴り拒否、native/wasm境界とpermission検査が`err`）、spec/wasm_cross/bytes_rawptr.almd（C-062、C-376）、tests/extern_rust_abi_test.rs（C ABI pairとsingle release）、scripts/check-rawptr-surface.py。

<a id="amendment-2026-10-29-regions-at-the-host-boundary"></a>

## 修訂（2026-10-29）: Host境界のRegion

epoch-15移行で約30 downstream repoがredになった。Regionをhostと交換できなかったためである。約20 fizz repoのJS driverとnendo-ceangal-web poolはexportにbuffer addressを求め、そこへ書き次exportを呼ぶ。ptcg-abcとbindgen scaffoldingはC bufferを`@export(c)` fnに渡す。ownerは2026-10-29に各穴へ回答した。拘束要件は回答5: inbound host dataはzero-copyかつboundedで、理想のため性能を犠牲にしない。長さは常にRegionから来て、境界で`Bytes` copyは起こらない。

<a id="a1-an-exportwasm-fn-may-return-a-region"></a>

### A1. `@export(wasm)` FnはRegionを返せる

wasm multi-value returnで`(ptr: i32, len: i32)`として越境する。Region引数がimportを越える`(base, length)` pair（D3）のwasm32形。

**所有権。** hostがmoduleの`__region_release` exportへ`ptr`を返すまでRegionを生存させる。詳細:

1. emitterのexport adapterがhost用にRegion blockのshareを1つ保持。program自身のcopyは通常規則でdropするが、host shareがある限りblockを解放できない。したがって`ptr`はdanglingにならず、host書込みbyteは後続callをまたいで同所に残る。
2. `__region_release(ptr)`でshareを返す。呼ばないhostはinstance寿命中blockを保持する。leakだがdangling pointerではない。
3. 空Regionは`(0, 0)`で越境し何も保持せず、0のreleaseはno-op。
4. sub-region（`ffi.slice`）もcopyなしにparent block内の自身のaddressとして越境可（owner○、2026-10-29）。adapterは渡した各`(ptr, block)`をmodule自身の小tableに登録（他globalの後に3つ: table block、count、capacity）。`__region_release(ptr)`がaddressを検索し、block shareを解放してentry削除。hostがsliceをreleaseするまでparentは生存する。既にrelease済みか未提供かを問わずtableにないaddressはtrap。
5. `effect fn` exportの`err`は`(ptr, len)`にslotを持たず、adapterがstderrへ書いてtrapする。

機構: IR rewrite（crates/almide-wasm/src/extern_region.rs、`wrap_region_exports`）がexportをcompiler生成wrapperへ移し、`ffi.__export_view(f(…))`を返す。これは1 shareを持つRegion blockと、その中のview offset/length。Region recordは通常所有規則でrelease。wrapperのstamp `region_export`が代わりのadapterをemitterに指示。adapter（crates/almide-wasm/src/region_export.rs）はwrapperを呼びview recordの殻を解放、addressをtableへ登録し、host用shareを保ちつつview data addressと長さを返す。

**幅**（owner○、2026-10-29）。両方向でtarget memoryのaddress幅を使う: wasm32は`(i32, i32)`、memory64は`i64`。emitterはaddressを`Int` scalarで計算。moduleが32bit memoryを宣言すると`imports::declare`はRegion importのbase/length parameterを`i32`とし（stubが位置を`address_params`にstamp）、stubの代わりにwrapするshimを置く。memory64ではshimなし、`i64`のまま。

<a id="a1b-an-exportwasm-fn-may-take-region-parameters-4354"></a>

### A1b. `@export(wasm)` FnはRegion Parameterを受け取れる（#4354）

hostは各Regionを`(base, len)`で渡す。Region import（D3）および#4353 Rust hostと同じpair、memoryのaddress幅。IR rewrite `extern_region::lend_region_params`が`__region_lend_<name>` wrapperを作り、Region位置を2値にしてexportを引き受ける（`address_params` stamp、emitter adapterは32bit memoryで`i32`を受け拡幅）。fnを`ffi.__lend(base, len)`で呼ぶ。module memoryの範囲を指すread-only Regionで、何も所有せずcall中だけ貸与。長さはA2のC caller同様hostの表明で、全accessはその長さに照合。E119はcall後保持可能なcopy（top-level `var`、return、closure）をA2同様拒否し、wasmにはcall後runtime backstopがない。通常hostはpush exportが渡した範囲を貸し返し、shareが生存を保つ。範囲がhost表明なのでread-only。writable inbound bufferはpush形式を使う。

<a id="a2-an-exportc-fn-may-take-region-parameters"></a>

### A2. `@export(c)` FnはRegion Parameterを受け取れる

Region parameterはC callerの`(const uint8_t *ptr, size_t len)`で越境しread-only。`@region(out = p)`で指定したparameterは`(uint8_t *ptr, size_t len)`、writableのcaller提供buffer形式。両方とも**call中だけ貸与**し、callerのmemoryなので何も所有しない。各accessをcaller長さと照合（D5: 範囲外`err`）。call後、例えばtop-level `var`などにRegion copyが残ればwrapperがprocessを停止する。callerが制御を取り戻し直ちにfreeできるbufferよりRegionが長生きすることを防ぐ。全copyから値を取り消せないため、Go `cgocheck`方式を常時適用する。これはbackstopであり、本体が書けるescapeはcheck時に拒否（owner○、2026-10-29、**E119**）。貸与Region、それを保持するlocal `let` / `var`、slice、containerをtop-level `var`へ保存、return、closure captureするとerror。checker型とlocal bindingを追跡する。他fnへ渡すRegionはruntime検査に委ねる。`check`は`@export(c)` fnのRegion returnをE090で拒否（callerがfreeできない）。Region parameter以外を指す`out`や、C bind/exportのどちらもしないfnの`@region`も拒否。長さが`@region(len = …)`同様callerの表明なので、`Built`行は各貸与Region（`@export(c) Region `p`
of `f``）を数える。

<a id="a3-ffislicer-off-n---resultregion-string"></a>

### A3. `ffi.slice(r, off, n) -> Result[Region, String]`

`r`内`off`から`n` byteのread-only view。`0 <= off`かつ`off + n <= ffi.len(r)`でなければ`err`、負countも`err`。parent memoryを読むのでwritable parentへの後の書込みが見える。parent ownerとrelease guardを保持し、memoryがviewより長生きする。parentがwritableでもviewは常にread-only。新provenanceを生成せずaddress算術もしない: Regionはblock内view offset（`off`、全生成点では0）を持ち、`read` / `write`が自身の境界検査後に加え、`@extern`には`base + off`を渡す。`@extern`へsliceを渡せばcopyなしsub-rangeとなり、nendo/ceangalのframeごとoutbound sub-range copyを除く。C-062を修訂して記す。

<a id="a4-zero-copy-inbound-host-data"></a>

### A4. Zero-CopyのInbound Host Data

- **Push。** hostはA1 exportが返したwritable Regionへ直接書き、次exportを呼ぶ。program自身のcopy（top-level `var`）から読む。
- **Pull。** `@extern(wasm)` / `@extern(c)` fnへ渡したwritable `ffi.alloc` Regionをcall中貸与（D3で既存）。foreign codeがその場で埋める。

<a id="how-others-cross-a-buffer-surveyed-2026-10"></a>

### 他言語のBuffer越境（2026-10調査）

| | Outbound view | Inbound、caller buffer | 境界をまたぐlifetime |
|---|---|---|---|
| **Rust** | `&[u8]` → `(ptr, len)`、`as_ptr` / `len`経由; 所有権譲渡は`Box<[u8]>::into_raw` | `extern "C" fn`の`slice::from_raw_parts(_mut)(ptr, len)`、長さはcaller表明を一度assert | call中borrow（実際は`'a`無制限）; `into_raw` / `from_raw`で所有権往復 |
| **Swift** 6.2 | `Span` / `RawSpan`（非escape view） | caller bufferには`MutableSpan` / `OutputSpan`; `withUnsafeMutableBytes` | `~Escapable`: compilerがcallよりviewの長生きを禁止、Almideは代わりにruntime検査（A2） |
| **Wasm component model** | canonical ABIが`list<u8>`をcallee memoryへcopy（`cabi_realloc`） | 同じcopy | `borrow<T>`はcallで終了、`own<T>`はdestructor実行; byte zero-copyなしでA1 share/A2 lend相当 |
| **Zig** | `[]u8` = `(ptr, len)`; `export fn f(p: [*]u8, n: usize)`と`p[0..n]` | `[*]u8` + `usize`、一度slice化 | 検査なし、sliceがmemoryより長生きしうる |

回答5がcomponent modelのcopyを除外するため、AlmideはRust/Zigの`(ptr, len)`越境を採る。lifetimeはcomponent modelの`own` / `borrow`分離を採り、A1は`own`同様shareを渡し、A2は`borrow`同様1call貸与。Swiftが非escapeを静的検査する所をAlmideはruntimeで検査する。checkerが非escape規則を持たないためである（下のscoped borrow代替案）。

<a id="what-stays-trusted-added"></a>

### 信頼し続けるもの（追加）

- `@export(c)` Regionに対するC callerの`(ptr, len)`。callごとの表明を宣言ごとに`Built`行で数える。
- wasm hostは線形メモリのどこでも書ける。A1が渡すのはshareと長さで、capabilityではない。hostへ渡すread-only Regionは契約上だけread-only。

<a id="read-narrowly-named-for-a-decision"></a>

### 限定解釈・裁定事項として明記

- `--host js`はA1 exportを`f(…) → [ptr, len]`（両幅ともnumber）でwrapし、`regionRelease(ptr)`を加える（#4354）。driverは`instance.exports.f()`から直接2数値を読んでもよい。
- C header generatorはない。A2 prototypeは手書き: `int32_t f(const uint8_t *buf, size_t buf_len)`、`out` parameterは`uint8_t *`。

証拠: crates/almide-wasm/tests/ffi_host_boundary.rs（wasmtime駆動のwasm A1、A4 push/pull）、tests/ffi_host_boundary_test.rs（native A2: read-only/`out` lend、bounds `err`、escape停止、E090規則）、tests/extern_rust_abi_test.rs（native A4 pull、`(uint8_t*, size_t)`）、tests/raw_ptr_surface_test.rsとspec/wasm_cross/bytes_rawptr.almd（両target A3、C-062）。

<a id="alternatives-considered"></a>

## 検討した代替案

- **`RawPtr`を残しallocation tableと照合。** runtimeが全提供blockを記録し`ffi.read`で検索。生成(1)しか覆わず、肝心のforeign pointerはtable外。accessごとlookup費用とtrusted runtime増大。却下。
- **何とも照合しない`ffi.read`の長さparameter。** 当時の設計そのもの、長さはcallerの表明。
- **Scoped borrow（`ffi.with_region(b, (r) => …)`、Swift `withUnsafeBytes`）。** 生成(1)には健全だがcheckerにない非escape closure規則が必要で、foreign pointerには答えない。D2のRC shareは新規則なしで同lifetimeを与える。
- **Hardware capability（CHERI）。** 最強の答えだがAlmideの出荷targetにない。
- **Raw memory全廃、C bindは`@extern(rust)` shimのみ。** 同じ未検査pointerをcheckerが読まない`native/*.rs`へ移すだけ。信頼基盤は縮まず増える。

<a id="consequences"></a>

## 帰結

- 利点: `ffi`経由の範囲外読み書きは両targetで`Err`。信頼基盤は全`ffi.read` / `ffi.write` callからforeign宣言ごと1長さ条項へ縮小。#3420/#3966の形を書けなくする。
- 費用: 型分離とABI変更（`Region`はC2引数）。pointer戻り値を持つ既存`@extern` bindingを全て書換える（`almide fix`で機械的に）。各accessに境界検査1回。
- `Handle` / `Region`分離でCM対応が直接的になる（`own<resource>` / `list<u8>`）。

<a id="falsifier"></a>

## 反証条件

- 外部関数が返すbuffer長がparameter/constant/returned out-valueのどれでもなく、`Handle`＋accessorにもbindできない実例。その場合`@region(len = …)`を豊かにするかD2.3が誤り。
- `ffi`を使う`research/benchmark/perf` workloadで境界検査costが5%を超える。

<a id="owner-decisions-"></a>

## 所有者の決定（○×）

各問い1決定、推奨付き。

1. **`RawPtr`を`Handle`（不透明、外部codeへ返すだけ）と`Region`（base,length,permission,owner、全access境界検査）へ分離。** 推奨○。調査対象は全て分離する（CM resource対`list`、Swift `OpaquePointer`対`Span`、Rust opaque pointer対slice）。
2. **外部pointer戻り値は`@extern`宣言が`@region(len = …)`で長さを記すときだけ`Region`、それ以外`Handle`。** 推奨○。Clang `__counted_by` / Swift safe C importをbindingへ移し、各長さ主張を数えられるtrusted行にする。
3. **`Region`はC ABIでpointerと`size_t`長さの2引数になり、calleeは常にruntime長を得る。** 推奨○。caller手書き長さを分離するとD1検査は境界で途切れる。
4. **`Bytes`からの`Region`はread-only、writable `Region`は`ffi.alloc(n)`のみ。** 推奨○。`let`への書込みを防ぎ値意味論を守る（#3966）。Rust `&[u8]`対`&mut [u8]`、CHERI permission bitと同じ区切り。
5. **範囲外`ffi.read` / `ffi.write`はabortでなく`Err`。** 推奨○。両方既に失敗channel付きeffect fnで、foreign長の誤りはcallerが扱えるruntime事実。list indexはabortのまま（C-067）、ffi表面限定。
6. **foreign `Region`のrelease関数を宣言`release = "…"`で指定し、最後のshare破棄時runtimeが呼ぶ。Almide codeはfreeしない。** 推奨○。CM `own` destructorで、Almide側のdouble-free/use-after-freeを除く。
7. **アドレスを`Int`にする綴り（`ffi.to_int`、`bytes.data_ptr`）を削除、wasm `@extern`へ`Region`を`(i32, i32)`で渡す。** 推奨○。残すと`Int`がaddressとしてhostへ達し長さ検査を迂回。
8. **D1–D7を1 dialect epoch、非推奨期間なしで導入、`almide fix`で旧綴り書換え。** 推奨○。不一致形は即除去というproject裁定に従い、`import ffi`の裏なので影響file集合が小さく可視。

<a id="references"></a>

## 参考資料

- Rust strict provenance（1.84で安定化）: https://doc.rust-lang.org/std/ptr/index.html#strict-provenance
- `slice::from_raw_parts`: https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html
- CHERI ISA (UCAM-CL-TR-987): https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-987.pdf ; CHERIoT: https://cheriot.org/
- Zigのpointerとslice: https://ziglang.org/documentation/master/#Pointers , https://ziglang.org/documentation/master/#Slices
- Swift SE-0447 `Span`: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0447-span-access-shared-contiguous-storage.md ; SE-0458 厳密メモリ安全性: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0458-strict-memory-safety.md
- Clang `-fbounds-safety`: https://clang.llvm.org/docs/BoundsSafety.html
- Go cgoのpointer受渡し規則: https://pkg.go.dev/cmd/cgo#hdr-Passing_pointers ; `runtime.Pinner`: https://pkg.go.dev/runtime#Pinner
- Component modelのcanonical ABIとresource: https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md , https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#handle-types
