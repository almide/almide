<a id="supervision--actors"></a>

# 監督とアクター

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](supervision-and-actors.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](supervision-and-actors.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](supervision-and-actors.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/supervision-and-actors.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されず、原本が引き続き権威を持ちます。歴史的な状態・実装・証明・監査・測定の主張は固定した原本の記述であり、ここで独立に実行・検証していません。未完の提案は現在の保証ではありません。コードブロックとコメントは原文どおり、原本メタデータは無効化した表示注記です。

> 原本の説明（無効化したメタデータ）: stdlib モジュールとしての Erlang 型アクター、supervisor、型付きチャネル。

<a id="overview"></a>

## 概要

Erlang/OTP に着想を得た、長寿命の並行プロセス、型付きメッセージ交換、障害回復のための未実装のライブラリ提案です。以下の例は設計スケッチであり、サポート済みの Almide API や確立された保証ではありません。

提案されたライブラリ境界は保持していますが、当初の前提だった `async fn` / `await` は現在の表面構文として採用されていません。復活させる場合は [ADR-0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) と [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) に照らして検討しなければならず、アクターの意味論が既に `fan` 契約の一部だと仮定してはなりません。

<a id="why-library-not-language"></a>

## 言語ではなくライブラリにする理由

| 論点 | 理由 |
|---------|--------|
| 複雑さの予算 | `process`、`supervisor`、`actor` をキーワードに加えると文法が肥大化する |
| 複数ターゲットのコスト | アクターランタイムには native とサポート対象の Wasm ホストへの明示的な対応づけが必要であり、この提案はいずれも提供していない |
| 使用頻度 | 多くのプログラムには supervision は不要だが、サービスやサーバーでは必要 |
| Almide の使命 | LLM の正確さに役立つのは小さな言語と豊富な stdlib であり、大きな言語ではない |

<a id="design-actor-module"></a>

## 設計: Actor モジュール

<a id="defining-an-actor"></a>

### アクターの定義

```almide
import { Actor, Msg, Reply } from "almide/actor"

type CounterMsg =
  | Increment
  | Decrement
  | Get(reply: Reply[Int])

fn counter_actor(ctx: Actor[CounterMsg]) -> Unit =
  var count = 0
  for msg in ctx.receive() {
    match msg {
      Increment -> { count = count + 1 }
      Decrement -> { count = count - 1 }
      Get(reply) -> reply.send(count)
    }
  }
```

<a id="spawning-and-messaging"></a>

### 起動とメッセージ交換

```almide
async fn main() -> Unit =
  do {
    let counter = await Actor.spawn(counter_actor)

    counter.send(Increment)
    counter.send(Increment)
    counter.send(Increment)

    let count = await counter.ask(|reply| Get(reply))
    print(count)  // 3
  }
```

<a id="key-api"></a>

### 主要 API

```almide
// Actor lifecycle
Actor.spawn[M](handler: fn(Actor[M]) -> Unit) -> ActorRef[M]
Actor.spawn_linked[M](handler: fn(Actor[M]) -> Unit) -> ActorRef[M]

// Messaging
ActorRef.send(msg: M) -> Unit              // fire-and-forget
ActorRef.ask[R](builder: fn(Reply[R]) -> M) -> R  // request-response

// Actor context
Actor.receive() -> Stream[M]               // message stream
Actor.self_ref() -> ActorRef[M]            // self reference
Actor.stop() -> Unit                       // graceful shutdown
```

<a id="design-supervisor-module"></a>

## 設計: Supervisor モジュール

<a id="declaring-supervision-trees"></a>

### 監督ツリーの宣言

```almide
import { Supervisor, ChildSpec, Strategy } from "almide/supervisor"

async fn start_app(config: Config) -> Unit =
  do {
    let sup = await Supervisor.start(
      strategy: Strategy.OneForOne,
      max_restarts: 5,
      within_seconds: 30,
      children: [
        ChildSpec {
          name: "db_pool",
          start: || DbPool.start(config.db),
        },
        ChildSpec {
          name: "cache",
          start: || Cache.start(config.redis),
        },
        ChildSpec {
          name: "api",
          start: || ApiServer.start(config.port),
        },
      ],
    )
    await sup.wait()
  }
```

<a id="supervision-strategies"></a>

### 監督戦略

| 戦略 | 挙動 |
|----------|----------|
| `OneForOne` | 失敗した子だけを再起動する |
| `OneForAll` | いずれかが失敗したら全子を再起動する |
| `RestForOne` | 失敗した子と、その後に起動したすべての子を再起動する |

<a id="key-api-1"></a>

### 主要 API

```almide
Supervisor.start(opts: SupervisorOpts) -> SupervisorRef
SupervisorRef.wait() -> Unit                    // block until shutdown
SupervisorRef.stop() -> Unit                    // graceful shutdown
SupervisorRef.which_children() -> List[ChildInfo]
```

<a id="design-channel-module"></a>

## 設計: Channel モジュール

バックプレッシャーを持つ、型付きの有界チャネル。

```almide
import { Channel } from "almide/channel"

async fn pipeline() -> Unit =
  do {
    let ch = Channel.new[LogEntry](buffer: 100)

    // producer
    concurrent {
      let producer = async {
        for entry in read_log_stream() {
          await ch.send(entry)  // blocks if buffer full
        }
        ch.close()
      }

      // consumer
      let consumer = async {
        ch.receive().for_each(|entry| process(entry))
      }
    }
  }
```

<a id="key-api-2"></a>

### 主要 API

```almide
Channel.new[T](buffer: Int) -> (Sender[T], Receiver[T])
Sender.send(value: T) -> Unit           // async, blocks when full
Sender.try_send(value: T) -> Result[Unit, Full]
Sender.close() -> Unit
Receiver.receive() -> Stream[T]         // async iterator
Receiver.try_receive() -> Result[T, Empty]
```

<a id="original-backend-sketch-historical-not-the-current-target-matrix"></a>

## 当初のバックエンド案（歴史的記録であり、現在のターゲット表ではない）

TypeScript 出力はもはやコンパイラのターゲットではありません。以下の TS 列は当初の選択肢だけを記録しています。Wasm が本質的にスレッドを持たないわけではなく、組み込みホストの worker サポート自体がこの提案のアクターランタイムを実装するわけでもありません。

| 概念 | Rust | TypeScript |
|---------|------|------------|
| Actor | `tokio::task` + `mpsc::channel` | `Worker` + `MessagePort`（Node）/ `postMessage`（browser） |
| Supervisor | 独自のランタイム crate（または `bastion`） | `child_process` によるプロセスマネージャー（Node）/ 該当なし（browser） |
| Channel | `tokio::sync::mpsc` | `ReadableStream` / 独自の非同期キュー |

<a id="platform-limitations"></a>

### プラットフォームの制約

- **WASM 提案**: 当初のスケッチは単一スレッド上の協調タスクを使っていました。これは提案するアクターランタイムの選択であり、現在のすべての Wasm ホストの制約ではありません。
- **Browser TS**: `child_process` はありません。Supervisor は error-boundary 方式の回復に縮退します。並列性には Web Workers を使います。
- **Deno/Node TS**: Worker スレッドを全面的にサポート。Supervisor は実現可能です。

<a id="implementation-phases"></a>

## 実装フェーズ

<a id="phase-1-channel"></a>

### Phase 1: Channel

- [ ] `Channel.new`、`Sender`、`Receiver` 型
- [ ] Rust codegen → `tokio::sync::mpsc`
- [ ] サポート対象の Wasm ホストへの対応づけを選ぶ。当初の TS 非同期キューのターゲットは廃止済み
- [ ] `spec/stdlib/channel_test.almd` 内のテスト

<a id="phase-2-actor"></a>

### Phase 2: Actor

- [ ] `Actor.spawn`、`ActorRef.send`、`ActorRef.ask`
- [ ] 非同期ストリームとしての `Actor.receive()`
- [ ] リンクされたアクター（障害の伝搬）
- [ ] `spec/stdlib/actor_test.almd` 内のテスト

<a id="phase-3-supervisor"></a>

### Phase 3: Supervisor

- [ ] `ChildSpec` を使う `Supervisor.start`
- [ ] `OneForOne`、`OneForAll`、`RestForOne` 戦略
- [ ] 最大再起動回数の追跡とエスカレーション
- [ ] `spec/stdlib/supervisor_test.almd` 内のテスト

<a id="phase-4-distributed-research"></a>

### Phase 4: 分散（研究）

- [ ] 位置透過な `ActorRef`（ローカルまたはリモート）
- [ ] ノードの発見と登録
- [ ] ネットワーク境界を越えるメッセージのシリアライズ
- [ ] このフェーズには相当な研究が必要であり、すべてのターゲットで実現できるとは限らない

<a id="dependencies"></a>

## 依存事項

- 当初の `async fn` / `await` スケッチを現在の effect/fan の表面と整合させる。以前の「DONE」ラベルは、アクターの前提機能が出荷済みであることを確立しない
- 現在の構造化された `fan` の意味論（上記リンクの ADR）との互換性を確立する。古い `concurrent` ブロックのスケッチは現在の API ではない
- 非同期ストリーム（`Stream[T]` 型）— `Actor.receive()` と `Channel.receive()` に必要

<a id="references"></a>

## 参考資料

| システム | 学ぶこと |
|--------|----------------------|
| **Erlang/OTP** | 監督ツリー、let-it-crash、プロセス分離、位置透過性 |
| **Akka (Scala)** | 型付きアクター、ask パターン、アクター階層 |
| **Bastion (Rust)** | Rust での Erlang 型の監督、軽量プロセス |
| **Orleans (.NET)** | 仮想アクター、自動的な活性化・非活性化 |
| **Cloudflare Durable Objects** | エッジ向けの状態を持つアクター、単一書き手の保証 |

<a id="status"></a>

## 状態

未着手。Layer 2（構造化並行性）に依存します。言語拡張ではなく stdlib モジュールとして設計しています。

## コード内説明・コメントの訳

コード順のコメント訳: カウンターの出力は 3。Actor API はライフサイクル、メッセージ交換、アクター文脈に分かれます。send は送りっぱなし、ask は要求・応答、receive はメッセージストリーム、self_ref は自身への参照、stop は正常終了です。Supervisor の wait は終了まで待機し、stop は正常終了です。チャネル例は producer（生産者）と consumer（消費者）を示し、send はバッファが満杯なら待機します。チャネル API の send は非同期で満杯時に待機、receive は非同期イテレーターと説明されています。これらは提案例へのコメントであり、現行 API の保証ではありません。

## 原本に関する注意

アクター・supervisor・チャネルの全例は、古い async/await と TS バックエンドの前提も含め、未実装の設計スケッチのままです。未チェックの各段階、研究上の不確実性、現在の fan 契約との整合の必要性を保持します。出荷済みアクターランタイム、サポート構文、障害隔離の保証、普遍的な Wasm スレッド制約を確立するものではありません。
