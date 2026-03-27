<!-- description: Erlang-style actors, supervisors, and typed channels as stdlib modules -->
# Supervision & Actors

## Overview

Layer 3 of Almide's async model. Provides long-lived concurrent processes, typed message passing, and automatic fault recovery — inspired by Erlang/OTP but with compile-time guarantees.

This layer is designed as a **library/stdlib extension**, not a language-level construct. It builds on Layer 1 (async/await) and Layer 2 (structured concurrency).

## Why Library, Not Language

| Concern | Reason |
|---------|--------|
| Complexity budget | Adding `process`, `supervisor`, `actor` as keywords bloats the grammar |
| Multi-target cost | Actor semantics differ vastly between Rust (OS threads/tokio tasks), TS (Workers), WASM (single-threaded) |
| Usage frequency | Most programs don't need supervision — services and servers do |
| Almide's mission | LLM accuracy is served by a small language + rich stdlib, not a large language |

## Design: Actor Module

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

## Design: Supervisor Module

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

### Supervision strategies

| Strategy | Behavior |
|----------|----------|
| `OneForOne` | Restart only the failed child |
| `OneForAll` | Restart all children if any fails |
| `RestForOne` | Restart the failed child and all children started after it |

### Key API

```almide
Supervisor.start(opts: SupervisorOpts) -> SupervisorRef
SupervisorRef.wait() -> Unit                    // block until shutdown
SupervisorRef.stop() -> Unit                    // graceful shutdown
SupervisorRef.which_children() -> List[ChildInfo]
```

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

### Key API

```almide
Channel.new[T](buffer: Int) -> (Sender[T], Receiver[T])
Sender.send(value: T) -> Unit           // async, blocks when full
Sender.try_send(value: T) -> Result[Unit, Full]
Sender.close() -> Unit
Receiver.receive() -> Stream[T]         // async iterator
Receiver.try_receive() -> Result[T, Empty]
```

## Codegen Strategy

| Concept | Rust | TypeScript |
|---------|------|------------|
| Actor | `tokio::task` + `mpsc::channel` | `Worker` + `MessagePort` (Node) / `postMessage` (browser) |
| Supervisor | Custom runtime crate (or `bastion`) | Process manager with `child_process` (Node) / not applicable (browser) |
| Channel | `tokio::sync::mpsc` | `ReadableStream` / custom async queue |

### Platform limitations

- **WASM**: No threads. Actors run as cooperative tasks on a single thread. Useful for state encapsulation but no true parallelism.
- **Browser TS**: No `child_process`. Supervisor degrades to error-boundary style recovery. Web Workers for parallelism.
- **Deno/Node TS**: Full Worker thread support. Supervisor viable.

## Implementation Phases

### Phase 1: Channel

- [ ] `Channel.new`, `Sender`, `Receiver` types
- [ ] Rust codegen → `tokio::sync::mpsc`
- [ ] TS codegen → async queue with backpressure
- [ ] Tests in `spec/stdlib/channel_test.almd`

### Phase 2: Actor

- [ ] `Actor.spawn`, `ActorRef.send`, `ActorRef.ask`
- [ ] `Actor.receive()` as async stream
- [ ] Linked actors (failure propagation)
- [ ] Tests in `spec/stdlib/actor_test.almd`

### Phase 3: Supervisor

- [ ] `Supervisor.start` with `ChildSpec`
- [ ] `OneForOne`, `OneForAll`, `RestForOne` strategies
- [ ] Max restart tracking and escalation
- [ ] Tests in `spec/stdlib/supervisor_test.almd`

### Phase 4: Distributed (research)

- [ ] Location-transparent `ActorRef` (local or remote)
- [ ] Node discovery and registration
- [ ] Message serialization across network boundaries
- [ ] This phase requires significant research and may not be viable for all targets

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE
- Layer 2 (structured concurrency) — required for concurrent actor patterns (see [Structured Concurrency](../active/structured-concurrency.md). `concurrent` block syntax is TBD)
- Async streams (`Stream[T]` type) — required for `Actor.receive()` and `Channel.receive()`

## References

| System | What to learn from it |
|--------|----------------------|
| **Erlang/OTP** | Supervision trees, let-it-crash, process isolation, location transparency |
| **Akka (Scala)** | Typed actors, ask pattern, actor hierarchy |
| **Bastion (Rust)** | Erlang-style supervision in Rust, lightweight processes |
| **Orleans (.NET)** | Virtual actors, automatic activation/deactivation |
| **Cloudflare Durable Objects** | Edge-native stateful actors, single-writer guarantee |

## Status

Not started. Depends on Layer 2 (structured concurrency). Designed as stdlib modules, not language extensions.
