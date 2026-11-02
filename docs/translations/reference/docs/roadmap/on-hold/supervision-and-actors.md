<a id="supervision--actors"></a>

# Supervision & Actors

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](supervision-and-actors.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](supervision-and-actors.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](supervision-and-actors.zh-CN.md)

> Non-normative reader copy/translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/supervision-and-actors.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Historical status, implementation, proof, audit and measurement claims remain statements of the pinned source; they have not been independently executed or validated here. Pending proposals are not current guarantees. Fences and comments remain verbatim; source metadata is displayed as an inactive note.

> Source description (inactive metadata): Erlang-style actors, supervisors, and typed channels as stdlib modules

<a id="overview"></a>

## Overview

Unimplemented library proposal for long-lived concurrent processes, typed
message passing and fault recovery, inspired by Erlang/OTP. The examples below
are design sketches, not supported Almide APIs or established guarantees.

The proposed library boundary is retained, but its original `async fn` / `await`
prerequisite was not adopted as the current surface. Any revival must be checked
against [ADR-0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md)
and [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md), not assume
that actor semantics are already part of the `fan` contract.

<a id="why-library-not-language"></a>

## Why Library, Not Language

| Concern | Reason |
|---------|--------|
| Complexity budget | Adding `process`, `supervisor`, `actor` as keywords bloats the grammar |
| Multi-target cost | An actor runtime would need an explicit mapping to native and supported Wasm hosts; neither mapping is supplied by this proposal |
| Usage frequency | Most programs don't need supervision — services and servers do |
| Almide's mission | LLM accuracy is served by a small language + rich stdlib, not a large language |

<a id="design-actor-module"></a>

## Design: Actor Module

<a id="defining-an-actor"></a>

### Defining an actor

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

### Spawning and messaging

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

### Key API

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

## Design: Supervisor Module

<a id="declaring-supervision-trees"></a>

### Declaring supervision trees

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

### Supervision strategies

| Strategy | Behavior |
|----------|----------|
| `OneForOne` | Restart only the failed child |
| `OneForAll` | Restart all children if any fails |
| `RestForOne` | Restart the failed child and all children started after it |

<a id="key-api-1"></a>

### Key API

```almide
Supervisor.start(opts: SupervisorOpts) -> SupervisorRef
SupervisorRef.wait() -> Unit                    // block until shutdown
SupervisorRef.stop() -> Unit                    // graceful shutdown
SupervisorRef.which_children() -> List[ChildInfo]
```

<a id="design-channel-module"></a>

## Design: Channel Module

Typed, bounded channels with backpressure.

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

### Key API

```almide
Channel.new[T](buffer: Int) -> (Sender[T], Receiver[T])
Sender.send(value: T) -> Unit           // async, blocks when full
Sender.try_send(value: T) -> Result[Unit, Full]
Sender.close() -> Unit
Receiver.receive() -> Stream[T]         // async iterator
Receiver.try_receive() -> Result[T, Empty]
```

<a id="original-backend-sketch-historical-not-the-current-target-matrix"></a>

## Original backend sketch (historical, not the current target matrix)

TypeScript output is no longer a compiler target. The TS column below records
the original alternatives only. Wasm is not inherently threadless: embedded
host worker support does not itself implement this proposed actor runtime.

| Concept | Rust | TypeScript |
|---------|------|------------|
| Actor | `tokio::task` + `mpsc::channel` | `Worker` + `MessagePort` (Node) / `postMessage` (browser) |
| Supervisor | Custom runtime crate (or `bastion`) | Process manager with `child_process` (Node) / not applicable (browser) |
| Channel | `tokio::sync::mpsc` | `ReadableStream` / custom async queue |

<a id="platform-limitations"></a>

### Platform limitations

- **WASM proposal**: the original sketch used cooperative tasks on a single thread. This is a proposed actor-runtime choice, not a restriction on all current Wasm hosts.
- **Browser TS**: No `child_process`. Supervisor degrades to error-boundary style recovery. Web Workers for parallelism.
- **Deno/Node TS**: Full Worker thread support. Supervisor viable.

<a id="implementation-phases"></a>

## Implementation Phases

<a id="phase-1-channel"></a>

### Phase 1: Channel

- [ ] `Channel.new`, `Sender`, `Receiver` types
- [ ] Rust codegen → `tokio::sync::mpsc`
- [ ] Choose a supported Wasm-host mapping; the original TS async-queue target is retired
- [ ] Tests in `spec/stdlib/channel_test.almd`

<a id="phase-2-actor"></a>

### Phase 2: Actor

- [ ] `Actor.spawn`, `ActorRef.send`, `ActorRef.ask`
- [ ] `Actor.receive()` as async stream
- [ ] Linked actors (failure propagation)
- [ ] Tests in `spec/stdlib/actor_test.almd`

<a id="phase-3-supervisor"></a>

### Phase 3: Supervisor

- [ ] `Supervisor.start` with `ChildSpec`
- [ ] `OneForOne`, `OneForAll`, `RestForOne` strategies
- [ ] Max restart tracking and escalation
- [ ] Tests in `spec/stdlib/supervisor_test.almd`

<a id="phase-4-distributed-research"></a>

### Phase 4: Distributed (research)

- [ ] Location-transparent `ActorRef` (local or remote)
- [ ] Node discovery and registration
- [ ] Message serialization across network boundaries
- [ ] This phase requires significant research and may not be viable for all targets

<a id="dependencies"></a>

## Dependencies

- Reconcile the original `async fn` / `await` sketches with the current effect/fan surface; no shipped actor prerequisite is established by the former “DONE” label
- Establish compatibility with current structured `fan` semantics (ADRs linked above). The old `concurrent` block sketch is not a current API.
- Async streams (`Stream[T]` type) — required for `Actor.receive()` and `Channel.receive()`

<a id="references"></a>

## References

| System | What to learn from it |
|--------|----------------------|
| **Erlang/OTP** | Supervision trees, let-it-crash, process isolation, location transparency |
| **Akka (Scala)** | Typed actors, ask pattern, actor hierarchy |
| **Bastion (Rust)** | Erlang-style supervision in Rust, lightweight processes |
| **Orleans (.NET)** | Virtual actors, automatic activation/deactivation |
| **Cloudflare Durable Objects** | Edge-native stateful actors, single-writer guarantee |

<a id="status"></a>

## Status

Not started. Depends on Layer 2 (structured concurrency). Designed as stdlib modules, not language extensions.

## Code-comment reading notes

In code order: the counter print is 3. The actor API groups lifecycle, messaging and actor context; send is fire-and-forget, ask is request-response, receive is a message stream, self_ref is a self reference, and stop is graceful shutdown. Supervisor wait blocks until shutdown and stop is graceful shutdown. The channel example labels producer and consumer; send blocks if the buffer is full. The channel API describes send as asynchronous and blocking when full, and receive as an asynchronous iterator. These are comments on proposed examples, not current API guarantees.

## Source caveats

All actor, supervisor and channel examples remain unimplemented design sketches, including their obsolete async/await and TS backend assumptions. The unchecked phases, research uncertainty, and need to reconcile with the current fan contract are retained. Nothing here establishes a shipped actor runtime, supported syntax, fault-isolation guarantee or universal Wasm threading restriction.
