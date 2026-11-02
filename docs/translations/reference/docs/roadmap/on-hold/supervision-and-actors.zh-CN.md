<a id="supervision--actors"></a>

# 监督与 Actor

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](supervision-and-actors.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](supervision-and-actors.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](supervision-and-actors.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/supervision-and-actors.md)的非规范性阅读译本，不会自动同步，原本仍具权威性。历史状态、实现、证明、审计和测量主张仍是固定原本的陈述，此处未经独立执行或验证。待完成的提案不是当前保证。代码块与注释原样保留，原本元数据作为已停用的注记显示。

> 原本说明（已停用元数据）: 作为 stdlib 模块的 Erlang 式 actor、supervisor 与类型化通道。

<a id="overview"></a>

## 概述

尚未实现的库提案，受 Erlang/OTP 启发，用于长生命周期并发进程、类型化消息传递及故障恢复。下方示例是设计草图，不是受支持的 Almide API，也不是已确立的保证。

保留提议的库边界，但原先的 `async fn` / `await` 前提未被采纳为当前语法表面。任何重启工作都必须对照 [ADR-0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) 和 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 检查，不能假定 actor 语义已是 `fan` 契约的一部分。

<a id="why-library-not-language"></a>

## 为什么是库而非语言

| 考量 | 理由 |
|---------|--------|
| 复杂度预算 | 把 `process`、`supervisor`、`actor` 加为关键字会膨胀语法 |
| 多目标成本 | actor 运行时需要到 native 和受支持 Wasm 主机的显式映射；本提案未提供这两种映射 |
| 使用频率 | 大多数程序不需要监督，服务与服务器才需要 |
| Almide 的使命 | 小语言加丰富 stdlib 有助于 LLM 准确性，而不是庞大的语言 |

<a id="design-actor-module"></a>

## 设计：Actor 模块

<a id="defining-an-actor"></a>

### 定义 actor

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

### 启动与消息传递

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

### 核心 API

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

## 设计：Supervisor 模块

<a id="declaring-supervision-trees"></a>

### 声明监督树

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

### 监督策略

| 策略 | 行为 |
|----------|----------|
| `OneForOne` | 仅重启失败的子项 |
| `OneForAll` | 任一子项失败则重启所有子项 |
| `RestForOne` | 重启失败的子项及在它之后启动的所有子项 |

<a id="key-api-1"></a>

### 核心 API

```almide
Supervisor.start(opts: SupervisorOpts) -> SupervisorRef
SupervisorRef.wait() -> Unit                    // block until shutdown
SupervisorRef.stop() -> Unit                    // graceful shutdown
SupervisorRef.which_children() -> List[ChildInfo]
```

<a id="design-channel-module"></a>

## 设计：Channel 模块

带背压的类型化有界通道。

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

### 核心 API

```almide
Channel.new[T](buffer: Int) -> (Sender[T], Receiver[T])
Sender.send(value: T) -> Unit           // async, blocks when full
Sender.try_send(value: T) -> Result[Unit, Full]
Sender.close() -> Unit
Receiver.receive() -> Stream[T]         // async iterator
Receiver.try_receive() -> Result[T, Empty]
```

<a id="original-backend-sketch-historical-not-the-current-target-matrix"></a>

## 原始后端草图（历史记录，而非当前目标矩阵）

TypeScript 输出已不再是编译器目标。下方 TS 列仅记录原始替代方案。Wasm 并非天生没有线程：嵌入式主机的 worker 支持本身并未实现本提案的 actor 运行时。

| 概念 | Rust | TypeScript |
|---------|------|------------|
| Actor | `tokio::task` + `mpsc::channel` | `Worker` + `MessagePort`（Node）/ `postMessage`（浏览器） |
| Supervisor | 自定义运行时 crate（或 `bastion`） | 使用 `child_process` 的进程管理器（Node）/ 不适用（浏览器） |
| Channel | `tokio::sync::mpsc` | `ReadableStream` / 自定义异步队列 |

<a id="platform-limitations"></a>

### 平台限制

- **WASM 提案**：原始草图采用单线程协作式任务。这是提议的 actor 运行时选择，不是对所有当前 Wasm 主机的限制。
- **浏览器 TS**：没有 `child_process`。Supervisor 降级为 error-boundary 式恢复。通过 Web Workers 实现并行。
- **Deno/Node TS**：完整支持 Worker 线程。Supervisor 可行。

<a id="implementation-phases"></a>

## 实现阶段

<a id="phase-1-channel"></a>

### Phase 1：Channel

- [ ] `Channel.new`、`Sender`、`Receiver` 类型
- [ ] Rust codegen → `tokio::sync::mpsc`
- [ ] 选择受支持的 Wasm 主机映射；原始 TS 异步队列目标已退役
- [ ] `spec/stdlib/channel_test.almd` 中的测试

<a id="phase-2-actor"></a>

### Phase 2：Actor

- [ ] `Actor.spawn`、`ActorRef.send`、`ActorRef.ask`
- [ ] `Actor.receive()` 作为异步流
- [ ] 关联 actor（故障传播）
- [ ] `spec/stdlib/actor_test.almd` 中的测试

<a id="phase-3-supervisor"></a>

### Phase 3：Supervisor

- [ ] 使用 `ChildSpec` 的 `Supervisor.start`
- [ ] `OneForOne`、`OneForAll`、`RestForOne` 策略
- [ ] 最大重启次数跟踪与升级处理
- [ ] `spec/stdlib/supervisor_test.almd` 中的测试

<a id="phase-4-distributed-research"></a>

### Phase 4：分布式（研究）

- [ ] 位置透明的 `ActorRef`（本地或远程）
- [ ] 节点发现与注册
- [ ] 跨网络边界的消息序列化
- [ ] 此阶段需要大量研究，可能无法适用于所有目标

<a id="dependencies"></a>

## 依赖事项

- 将原始 `async fn` / `await` 草图与当前 effect/fan 表面对齐；原先的“DONE”标签不能确立任何已发布的 actor 前置条件
- 确立与当前结构化 `fan` 语义的兼容性（上方链接的 ADR）。旧 `concurrent` 块草图不是当前 API
- 异步流（`Stream[T]` 类型）— `Actor.receive()` 和 `Channel.receive()` 所需

<a id="references"></a>

## 参考资料

| 系统 | 可借鉴之处 |
|--------|----------------------|
| **Erlang/OTP** | 监督树、let-it-crash、进程隔离、位置透明性 |
| **Akka (Scala)** | 类型化 actor、ask 模式、actor 层次 |
| **Bastion (Rust)** | Rust 中的 Erlang 式监督、轻量进程 |
| **Orleans (.NET)** | 虚拟 actor、自动激活/停用 |
| **Cloudflare Durable Objects** | 边缘原生的有状态 actor、单写者保证 |

<a id="status"></a>

## 状态

尚未开始。依赖 Layer 2（结构化并发）。设计为 stdlib 模块，而非语言扩展。

## 代码说明与注释译文

按代码顺序翻译注释：计数器输出为 3。Actor API 分为生命周期、消息传递和 actor 上下文；send 是发送后不等待，ask 是请求—响应，receive 是消息流，self_ref 是自身引用，stop 是优雅关闭。Supervisor 的 wait 阻塞至关闭，stop 是优雅关闭。通道示例标注生产者与消费者；缓冲区满时 send 阻塞。通道 API 说明 send 为异步且满时阻塞，receive 为异步迭代器。这些是提议示例的注释，不是当前 API 保证。

## 原本注意事项

所有 actor、supervisor 和 channel 示例仍是未实现的设计草图，包括其过时的 async/await 与 TS 后端假设。保留未勾选阶段、研究不确定性及与当前 fan 契约对齐的必要性。本文不确立已发布的 actor 运行时、受支持语法、故障隔离保证或普遍的 Wasm 线程限制。
