<!-- description: SolidJS-like reactive UI framework built as a pure Almide library -->
<a id="almide-ui--reactive-web-framework-as-almide-library"></a>

# Almide UI — AlmideライブラリとしてのリアクティブWebフレームワーク

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-ui.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-ui.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-ui.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-ui.md)
> 保留中の履歴提案であり、動作するライブラリではない。元コード、退役TSスケッチ、比較図は例示として保持し、現行の実行可能構文や新benchmark結果とは扱わない。10–15宣言でDOM全体をカバー、V8最高速、高い確信度等は原文の設計主張であり、host/ownership/最適化の未解決前提も併記する。実測優位性やzero-overhead保証をここで確立していない。

<a id="thesis"></a>

## 主張

提案：可能な限りフレームワーク固有の知識をコンパイラの外に置き、AlmideでリアクティブUIライブラリを作る。当初の設計が依存したTS出力は退役している。対応するbrowser host／DOM bridgeの選択が現在の前提条件であり、Svelte/SolidJS級の性能も、コンパイラ無変更での実現可能性も実証していない。以下のコードは当初の設計スケッチを保持するもので、現在動作するAlmideフレームワークではない。

```
What the user writes: Almide builder syntax + signal library
Original backend assumption (retired): optimize Almide code → TS output
Desired result: no virtual DOM, fine-grained updates; current host mapping unresolved
```

**設計目標：フレームワークの知識をライブラリ内に保つ。**コンパイラ作業が不要と主張する前に、host相互運用と最適化の対応範囲を確立する必要がある。

<a id="why-this-is-the-right-approach"></a>

## なぜこの方式が適切か

<a id="comparison-with-other-approaches"></a>

### 他方式との比較

| 方式 | 例 | 問題 |
|---|---|---|
| コンパイラ組込みフレームワーク | Svelte | コンパイラが複雑化。フレームワークの進化＝コンパイラの手直し |
| JS runtimeライブラリ | React, Vue | virtual DOM差分計算のruntimeコスト、bundleサイズ |
| JSライブラリ＋細粒度 | SolidJS | 最速級。ただしJSの型情報不足が最適化の上限を制限 |
| **Almideライブラリ** | **本提案** | SolidJSモデル＋Almideの型情報による追加最適化 |

<a id="benefits-of-being-an-almide-library"></a>

### Almideライブラリである利点

1. **コンパイラ変更を最小化** — ライブラリだけで十分か判断する前に既存言語／host対応を評価する
2. **フレームワークもAlmideで書く** — ユーザーコードと同じ言語、同じ最適化対象
3. **型情報がフレームワーク内部まで届く** — `Signal[Int]`の`.get()`がIntを返すと確定する。型特殊化が最深部まで届く
4. **フレームワークの進化はコンパイラから独立** — ライブラリ更新にコンパイラのリリースは不要
5. **複数ターゲットの利点** — `--target rust`でフレームワークのテストを高速実行できる

<a id="architecture"></a>

## アーキテクチャ

<a id="layer-structure"></a>

### 層構成

```
almide-ui/                    ← Almide package (library)
├── reactive.almd             Signal, Effect, Derived
├── dom.almd                  DOM primitives (bridged via @extern)
├── component.almd            Component lifecycle
├── renderer.almd             Signal→DOM update connection
└── html.almd                 builder Html + element functions (div, p, ...)
```

<a id="reactive-core"></a>

### Reactive core

```almide
// reactive.almd

type Signal[T] = {
  var value: T,
  var subs: List[fn() -> Unit],
}

fn signal[T](initial: T) -> Signal[T] =
  { value: initial, subs: [] }

fn get[T](s: Signal[T]) -> T = s.value

fn set[T](s: Signal[T], new_val: T) -> Unit = {
  s.value = new_val
  for sub in s.subs { sub() }
}

fn subscribe[T](s: Signal[T], f: fn() -> Unit) -> Unit =
  s.subs = s.subs ++ [f]

fn derived[T](compute: fn() -> T) -> Signal[T] = {
  // Automatically tracks dependent signals and returns computed value
  // Equivalent to SolidJS's createMemo
  ...
}
```

これは提案された表現である。1行の本体だけでinliningは保証されない。現行inlinerにはscalar、scope、ownershipの受入規則がある。zero overheadを主張する前に表現と生成コードを検証する。

<a id="dom-bridge"></a>

### DOM bridge

```almide
// dom.almd — minimal @extern bridge

@extern(ts, "document.createElement(tag)")
fn create_element(tag: String) -> DomNode

@extern(ts, "node.textContent = text")
fn set_text(node: DomNode, text: String) -> Unit

@extern(ts, "parent.appendChild(child)")
fn append(parent: DomNode, child: DomNode) -> Unit

@extern(ts, "node.addEventListener(event, handler)")
fn on(node: DomNode, event: String, handler: fn() -> Unit) -> Unit

@extern(ts, "node.setAttribute(name, value)")
fn set_attr(node: DomNode, name: String, value: String) -> Unit
```

10–15個の`@extern`宣言でDOM APIを完全にカバーできる。フレームワークの仕事はそれらを組み合わせて宣言的APIを提供することだ。

<a id="renderer--connecting-signals-to-the-dom"></a>

### Renderer — SignalとDOMの接続

```almide
// renderer.almd

fn reactive_text(parent: DomNode, s: Signal[String]) -> Unit = {
  let node = create_element("span")
  set_text(node, s.get())
  append(parent, node)
  // When signal changes, only update textContent
  s.subscribe(fn() => set_text(node, s.get()))
}

fn reactive_attr(node: DomNode, name: String, s: Signal[String]) -> Unit = {
  set_attr(node, name, s.get())
  s.subscribe(fn() => set_attr(node, name, s.get()))
}

fn reactive_list[T](
  parent: DomNode,
  items: Signal[List[T]],
  render: fn(T) -> DomNode,
) -> Unit = {
  // List diff update (keyed reconciliation)
  ...
}
```

<a id="user-code"></a>

### ユーザーコード

```almide
import almide_ui exposing (signal, get, set, derived)
import almide_ui/html exposing (div, h1, p, button, ul, li)

var count = signal(0)
let doubled = derived(fn() => count.get() * 2)

template app() -> HtmlDoc = Html {
  div(class: "app") {
    h1 { "Counter" }
    p { "Count: ${count.get()}" }
    p { "Doubled: ${doubled.get()}" }
    button(onclick: fn() => count.set(count.get() + 1)) {
      "Increment"
    }
  }
}
```

ユーザーが知る必要があるのはSignalとbuilder構文だけ。DOM操作は見えない。

<a id="compiler-optimization--what-happens-automatically"></a>

## コンパイラ最適化 — 自動的に何が起きるか

以下は望まれる最適化の例であり、この未実装Signal APIに対して保証された変換ではない：

<a id="1-inlining"></a>

### 1. Inlining

```almide
// Definition of signal.get()
fn get[T](s: Signal[T]) -> T = s.value

// User code
p { "Count: ${count.get()}" }

// After inlining (compiler internal)
p { "Count: ${count.value}" }

// Original illustrative TS output (retired target)
p.textContent = `Count: ${count.value}`;
```

提案された`.get()` / `.set()`最適化には生成コードとruntimeの測定が必要。関数サイズだけではSignalのzero overheadは確立しない。

<a id="2-type-specialization-monomorphization"></a>

### 2. 型特殊化（単相化）

```almide
// Generic definition
fn get[T](s: Signal[T]) -> T = s.value

// Used with Signal[Int] → Int-specialized version is generated
// Used with Signal[String] → String-specialized version is generated
```

V8は単相のcall siteを最高速で処理する。型特殊化により、すべてのSignal操作が単相になる。

<a id="3-pipeline-fusion"></a>

### 3. Pipeline fusion

```almide
let visible_names = items.get()
  |> list.filter((x) => x.active)
  |> list.map((x) => x.name)
```

中間配列なしの単一loopへ融合する。リアクティブlist更新が速くなる。

<a id="4-dead-code-elimination"></a>

### 4. 不要コード削除

未使用のSignal methodとDOM helperを出力から削除し、bundleサイズを最小化する。

<a id="future-option-compiler-aware-signal-tracking"></a>

## 将来の選択肢：コンパイラがSignalを認識して追跡

Signalを認識するpassは任意の研究方向である。ライブラリのみの性能同等性も、そのpassの必要性も確立していない：

```almide
// Compiler tracks Signal.get() calls inside builder blocks
Html {
  p { "Count: ${count.get()}" }     // ← depends on count
  p { "Name: ${name.get()}" }      // ← depends on name
  p { "Static text" }               // ← no dependencies
}
```

コンパイラが依存関係を見て、更新関数を自動生成する：

```typescript
// create
const p0 = document.createElement('p');
p0.textContent = `Count: ${count.value}`;
const p1 = document.createElement('p');
p1.textContent = `Name: ${name.value}`;
const p2 = document.createElement('p');
p2.textContent = 'Static text';

// update — when count changes, only p0 updates
count.subscribe(() => { p0.textContent = `Count: ${count.value}`; });
// update — when name changes, only p1 updates
name.subscribe(() => { p1.textContent = `Name: ${name.value}`; });
// p2 has no subscription (static, no update needed)
```

runtimeで依存関係を追跡する実装も別候補である。それとコンパイラ支援方式のどちらを選ぶか決める前に、正しさ、host統合、コストの測定が必要。

<a id="competitive-position"></a>

## 競争上の位置付け

```
                    Compile-time analysis  Runtime cost       Ecosystem
Svelte              ◎ (compiler)           ◎ (minimal)        ○ (JS)
SolidJS             △ (JSX transform)      ◎ (signal)         ○ (JS)
React               × (runtime diff)       △ (vDOM)           ◎ (largest)
Leptos (Rust→WASM)  ○ (signal)             ○ (WASM overhead)  △ (WASM FFI)
Almide UI proposal  unmeasured           unmeasured         host mapping unresolved
```

Almide UIの差別化点：
- **クライアント出力** — TypeScript backendの退役後は未決。元のplain-JS性能／互換性仮説は出荷済み能力でも実測結果でもない
- **SSR／クライアントコード共有** — 目標であり、現在のTS出力ではない。対応client hostと境界は設計が必要
- **型駆動最適化** — 提案上の機会であり、SolidJSに対する確立済み優位性ではない
- **ライブラリ境界** — 設計目標。必要なコンパイラ統合の特定はまだ必要

<a id="relationship-to-other-roadmap-items"></a>

## 他のroadmap項目との関係

- **元のTS emitter roadmap**：履歴上の入力のみであり、利用可能な出力ターゲットではない
- **Result Builder（template.md）**：`builder Html`はこのフレームワークのUI記述基盤そのもの。builder機構が完成すればAlmide UIのDOM構築は自然に動く
- **cross-target semantics**：SSR／client adapterには明示的な互換性テストが必要。旧TS計画の仮定を引き継ぐだけでは足りない
- **Wasm／browser host**：置換先の可能性であり、選定済みまたは実装済みUI backendではない

<a id="prerequisites"></a>

## 前提条件

1. **builder機構（template.md Phase 1）** — `builder Html`とtrailing blockがなければUI記述が書けない
2. **対応client backend** — browser host／DOM境界を選び検証する。旧TS emitter前提は退役
3. **genericsの安定性** — `Signal[T]`には正しい型検査とcodegenが必要
4. **closure／ownership semantics** — 選択したhost上で購読の寿命とcallbackを検証する

<a id="why-on-hold"></a>

## 保留中の理由

client backend、DOM境界、builder構文、購読のownershipが設計上の依存事項として残る。元のlibrary-firstの論拠を保持するが、それだけではこれらの問題は閉じない：

- **設計作業が残る** — Signalのスケッチ、extern宣言、builder提案だけで動作するDOM統合は確立しない
- **コンパイラの範囲は未決** — フレームワーク固有作業を選ぶ前に現行generic最適化の対応範囲を測定する
- **段階的に構築可能** — reactive core → DOM bridge → renderer → builder統合、各段階は独立
- **先行事例による動機** — library-firstの細粒度reactivityは評価する価値があるが、他所での存在は本提案のbenchmarkではない

確信度は高い。前提となる言語機能が揃えば、フレームワーク自体はAlmideコードにすぎない。

### 保存したコード・図の説明訳

コードとそのコメントは原文byteを保持する。冒頭図は、ユーザー入力がAlmide builder構文＋signalライブラリ、旧backend仮定がAlmide最適化→TS出力（退役）、目標がvirtual DOMなしの細粒度更新であり現host対応は未決、と示す。層図はreactive.almdがSignal/Effect/Derived、dom.almdがextern bridgeによるDOM primitive、component.almdがcomponent寿命、renderer.almdがSignal→DOM更新接続、html.almdがbuilder Htmlと要素関数を担当する構想。derivedのコメントは依存Signalを自動追跡して計算値を返し、SolidJS createMemo相当、と説明する。rendererのコメントはSignal変更時にtextContentだけを更新し、listにはkeyed reconciliationを使うという案である。

inlining例のコメントはget定義、ユーザーコード、compiler内部でのinline後、退役TS出力の例を順に示す。単相化例はgeneric定義をSignal[Int]とSignal[String]で使い、それぞれInt/String特殊化版を生成する構想。追跡例はbuilder内get呼び出しからcount依存、name依存、依存なしを判別する案。生成TS例はp0/p1/p2を作り、count変更時はp0のみ、name変更時はp1のみ更新し、静的p2には購読が不要と示す。

競争図の列はコンパイル時解析・実行時コスト・ecosystem。元図の評価記号を保持し、Svelteはcompiler／最小／JS、SolidJSはJSX変換／signal／JS、Reactはruntime差分／vDOM／最大ecosystem、Leptosはsignal／WASM overhead／WASM FFIとする原文比較。Almide UIは解析・実行コストとも未測定でhost対応未決。これは独立した現行比較ではない。
