Original description metadata: Wrap external Rust/TS/Python code as native Almide packages via @extern
<a id="rainbow-bridge--wrap-external-code-as-almide-packages"></a>

# Rainbow Bridge — Wrap External Code as Almide Packages

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](rainbow-bridge.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](rainbow-bridge.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](rainbow-bridge.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rainbow-bridge.md)

> Source caveat: this on-hold automation proposal is not shipped bridge tooling or a verified code tutorial. Fences preserve old syntax and incomplete examples (including undeclared/import-missing names) and were not compiled. Type-safe wrappers do not make motor selection, state-transition behavior or foreign code infallible; exhaustive matching is not proof of correct physical behavior. The original native manifest and TS fallback are superseded sketches, not permissions or supported-target guidance. ABI conversion, string lifetime/ownership and errors require a reviewed contract; native code, dependencies and build scripts remain trusted. Existing package/native support does not imply a general safe bridge generator.


> This is an unimplemented bridge-automation design, not a current package/FFI
> manual. Its code and `[native]` manifest are original sketches. For shipped
> package resolution, native dependencies and permission checks, use
> [package-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md) and
> [CLI install/deps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md#almide-install).

<a id="thesis"></a>

## Thesis

Wrap external code such as Rust crates, npm packages, and Python libraries as Almide packages, presented to users so they **look like ordinary Almide libraries**. Same as how Flutter wraps iOS/Android native code as plugins.

```
Flutter:    Swift/Kotlin native code → Flutter Plugin → import and use
Almide:     Rust/TS/Python/Ruby/etc. → Rainbow Bridge → import and use
```

Users just write `import robot_driver`. They don't need to know what language it's written in.

<a id="why-this-matters"></a>

## Why This Matters

<a id="there-are-domains-almide-cant-reach-alone"></a>

### There Are Domains Almide Can't Reach Alone

- Robot control (GPIO, motors, real-time control) → Rust crate
- Image processing / computer vision (OpenCV, etc.) → Rust/C++ crate
- ML inference (ONNX Runtime, TensorFlow Lite, etc.) → Python library
- Cryptography / authentication (HSM, TPM, OS keychain) → Rust crate
- Database drivers (PostgreSQL wire protocol, etc.) → Rust/TS package
- OS-specific APIs (Bluetooth, USB, serial communication) → Rust crate
- Web frameworks (Hono, Express, etc.) → npm package
- Data science (NumPy, pandas, etc.) → Python library

Each language ecosystem already has high-quality implementations. Reimplementing in Almide is wasteful. **Just wrapping should be enough to use them.**

<a id="division-of-labor-almide-is-the-safe-layer-external-code-is-the-implementation-layer"></a>

### Division of Labor: "Almide Is the Safe Layer, External Code Is the Implementation Layer"

```
┌─────────────────────────────────────────┐
│  Almide                                 │
│  State transitions, business logic,     │
│  data processing, pattern matching,     │
│  pipelines, type safety                 │
├─────────────────────────────────────────┤
│  Rainbow Bridge (@extern + type defs)    │
│  Wrap Rust API with Almide types        │
├─────────────────────────────────────────┤
│  Rust crate                             │
│  Hardware control, real-time processing │
│  Low-level I/O, performance-critical    │
└─────────────────────────────────────────┘
```

Typed Almide wrappers do not sandbox the native implementation. Native sources,
Cargo dependencies and their build scripts are additional trust boundaries;
`@extern` is not the only way native code enters a build. The proposed bridge
must preserve the current dependency-native permission and install gates.

<a id="package-structure"></a>

## Package Structure

```
almide-robot-driver/
├── almide.toml                 # Package definition + native dependency declarations
├── src/
│   ├── motor.almd              # Almide API — wrapped via @extern
│   ├── sensor.almd             # Almide API — wrapped via @extern
│   └── types.almd              # Type definitions — pure Almide
└── native/
    ├── Cargo.toml              # Rust crate
    └── src/
        └── lib.rs              # Rust implementation (#[no_mangle] extern "C" fn)
```

<a id="almidetoml"></a>

### almide.toml

```toml
[package]
name = "robot-driver"
version = "0.1.0"

[native]
type = "rust"
path = "native/"                # Location of Rust crate
# Or reference external crate directly
crate = "robot-hal"
version = "0.3"
```

<a id="srctypesalmd--type-definitions-are-pure-almide"></a>

### src/types.almd — Type definitions are pure Almide

```almide
type MotorId = | Left | Right | Arm | Gripper

type Speed = Speed(Float)

type SensorData = {
  distance: Float,
  angle: Float,
  confidence: Float,
  timestamp: Int,
}

type RobotState =
  | Idle
  | Moving { target: Position, speed: Speed }
  | Grabbing { object: ObjectId }
  | Error { code: Int, message: String }

type Position = { x: Float, y: Float, z: Float }
type ObjectId = ObjectId(Int)
```

The design keeps type definitions in Almide. Automatic conversion to/from
Rust-side types is a proposed bridge feature, not established by the type
checker or by the existing native-dependency integration.

<a id="srcmotoralmd--api-wrapping"></a>

### src/motor.almd — API wrapping

```almide
import types exposing (MotorId, Speed)

// Bridge to Rust implementation
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit

@extern(rs, "robot_driver_native", "motor_stop")
effect fn raw_stop(motor_id: Int) -> Unit

@extern(rs, "robot_driver_native", "motor_get_position")
effect fn raw_get_position(motor_id: Int) -> Float

// ---- User-facing API (Almide-idiomatic type-safe interface) ----

fn motor_to_id(motor: MotorId) -> Int =
  match motor {
    Left => 0,
    Right => 1,
    Arm => 2,
    Gripper => 3,
  }

effect fn set_speed(motor: MotorId, speed: Speed) -> Unit = {
  let Speed(v) = speed
  raw_set_speed(motor_to_id(motor), v)
}

effect fn stop(motor: MotorId) -> Unit =
  raw_stop(motor_to_id(motor))

effect fn stop_all() -> Unit = {
  stop(Left)
  stop(Right)
  stop(Arm)
  stop(Gripper)
}

// Pure utility functions
fn speed_percent(percent: Int) -> Speed =
  Speed(float.from_int(percent) / 100.0)

fn speed_clamp(speed: Speed, max: Speed) -> Speed = {
  let Speed(v) = speed
  let Speed(m) = max
  Speed(math.min(v, m))
}
```

<a id="srcsensoralmd"></a>

### src/sensor.almd

```almide
import types exposing (SensorData)

@extern(rs, "robot_driver_native", "sensor_read")
effect fn raw_read(sensor_id: Int) -> String  // Pass as JSON string

effect fn read_sensor(id: Int) -> SensorData = {
  let json_str = raw_read(id)
  json.decode[SensorData](json_str) |> unwrap_or(SensorData {
    distance: 0.0, angle: 0.0, confidence: 0.0, timestamp: 0
  })
}

effect fn read_all_sensors(ids: List[Int]) -> List[SensorData] =
  ids |> list.map(read_sensor)

fn closest(readings: List[SensorData]) -> Option[SensorData] =
  readings
    |> list.filter((r) => r.confidence > 0.5)
    |> list.min_by((r) => r.distance)
```

<a id="user-experience"></a>

## User Experience

User code. Unaware of Rust's existence:

```almide
import robot_driver exposing (set_speed, stop_all, speed_percent, read_sensor, closest)
import robot_driver/types exposing (MotorId, RobotState, SensorData)

fn decide(state: RobotState, sensors: List[SensorData]) -> RobotState =
  match (state, closest(sensors)) {
    (Idle, Some(s)) if s.distance < 1.0 =>
      Moving { target: { x: s.distance, y: 0.0, z: 0.0 }, speed: speed_percent(30) },
    (Moving { .. }, Some(s)) if s.distance < 0.3 =>
      Idle,
    (state, _) => state,
  }

effect fn control_loop() -> Unit = {
  var state = Idle
  loop {
    let sensors = read_all_sensors([0, 1, 2])
    state = decide(state, sensors)
    match state {
      Moving { speed, .. } => {
        set_speed(Left, speed)
        set_speed(Right, speed)
      },
      Idle => stop_all(),
      _ => {},
    }
  }
}
```

**Robot state transitions are written with Almide pattern matching. Motor control runs Rust behind the scenes. Users only need to know Almide.**

<a id="build-integration"></a>

## Build Integration

```bash
almide build app.almd
```

Compiler dependency resolution flow:

```
1. Read almide.toml
2. robot-driver is a dependency
3. robot-driver/almide.toml has a [native] section
4. Build native/Cargo.toml with cargo build --release
5. Add generated .a / .so to link targets
6. Almide's @extern references native FFI functions
7. Link into final binary
```

Users just run `almide build`. No need to know `cargo`.

<a id="extern-rust-side-convention"></a>

## @extern Rust-Side Convention

Functions exported by the native crate use C ABI:

```rust
// native/src/lib.rs

#[no_mangle]
pub extern "C" fn motor_set_speed(motor_id: i64, speed: f64) {
    // Hardware control implementation
}

#[no_mangle]
pub extern "C" fn motor_stop(motor_id: i64) {
    // ...
}

#[no_mangle]
pub extern "C" fn sensor_read(sensor_id: i64) -> *const c_char {
    // Returns JSON string (decoded on Almide side)
}
```

<a id="future-auto-generated-bridge"></a>

### Future: Auto-Generated Bridge

Auto-generate Almide `@extern` definitions from Rust crate `pub fn` signatures:

```bash
almide bridge generate --from native/Cargo.toml --out src/generated_bridge.almd
```

```almide
// src/generated_bridge.almd (auto-generated)
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn motor_set_speed(motor_id: Int, speed: Float) -> Unit

@extern(rs, "robot_driver_native", "motor_stop")
effect fn motor_stop(motor_id: Int) -> Unit
```

Rust crates become usable from Almide without manual wrappers. However, the user-facing type-safe API (MotorId variant → Int conversion, etc.) is written by hand. This is what gives it the "Almide touch."

<a id="the-almide-touch--what-value-does-wrapping-add"></a>

## The Almide Touch — What Value Does Wrapping Add?

The value of wrapping in Almide rather than using Rust crates directly:

**1. Type-safe API**

```rust
// Rust: motor_id is i64. Can't tell if 0 is Left or Right
fn motor_set_speed(motor_id: i64, speed: f64)
```

```almide
// Almide: MotorId is a variant. Impossible to get wrong
effect fn set_speed(motor: MotorId, speed: Speed) -> Unit
```

**2. State Transitions via Pattern Matching**

The Rust driver only provides a low-level API. Writing robot behavior logic on the Almide side with variant + match guarantees exhaustive state transition coverage at compile time.

**3. Data Processing via Pipelines**

```almide
let obstacles = read_all_sensors([0, 1, 2, 3])
  |> list.filter((s) => s.confidence > 0.8)
  |> list.filter((s) => s.distance < 2.0)
  |> list.sort_by((s) => s.distance)
```

**4. I/O Tracking via effect fn**

The `decide` function is a pure fn. It just takes sensor data and returns the next state. Easy to test:

```almide
test "obstacle avoidance" {
  let sensors = [SensorData { distance: 0.2, angle: 0.0, confidence: 0.9, timestamp: 0 }]
  let next = decide(Moving { target: origin, speed: speed_percent(50) }, sensors)
  assert_eq(next, Idle)
}
```

Logic can be tested without hardware. The separation of effect fn and pure fn makes this possible.

**5. Serialization via Codec**

Robot configuration, logs, and communication messages are automatically converted to JSON/binary via Almide's Codec:

```almide
let config = json.encode(robot_config)   // Config to JSON
let state = json.decode[RobotState](msg) // Restore state from message
```

**6. Web dashboard (original target idea, now unresolved)**

The original `--target ts` sharing plan is unavailable: TypeScript output has
been retired. A browser interface needs a supported Wasm/host or serialization
boundary, with separately tested compatibility; it is not an existing
cross-target type-consistency guarantee.

<a id="capability-integration"></a>

## Capability Integration

Original proposed manifest (not the implemented permission syntax):

```toml
# User's almide.toml
[dependencies.robot-driver]
version = "0.1.0"
capabilities = ["hardware"]    # Explicitly grant hardware access
```

This `capabilities = ["hardware"]` sketch is not the current gate. Current
package dependency native code is controlled by `[permissions].native`;
`almide deps` reports it, and `almide install` separately requires
`--allow-native` for the installed root package's native code. Capability
analysis treats `@extern` conservatively; it does not prove foreign code safe.
See the linked current package/CLI specifications for exact scope.

<a id="original-ts-fallback-alternatives-historical-sketches"></a>

## Original TS fallback alternatives (historical sketches)

There is no supported `--target ts`. The alternatives below record the original
design discussion; they are not current target behavior or runnable guidance:

**Option A: Compile error**
```
error: robot-driver requires rainbow bridge (Rust), but target is ts
  = hint: use --target rust, or mock the driver for testing
```

**Option B: Provide mock / stub**
```almide
// src/motor.almd — Write fallback body alongside @extern
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit =
  println("MOCK: set_speed motor=${int.to_string(motor_id)} speed=${float.to_string(speed)}")
```

This sketch does not establish fallback behavior for a retired TS target.
Any supported-target fallback or mock strategy must be tested against the
actual extern implementation rather than inferred from these alternatives.

<a id="relationship-to-other-roadmap-items"></a>

## Relationship to Other Roadmap Items

- **rainbow-gate.md**: Opposite direction. Rainbow FFI Gate is Almide → external (publish Almide as a library). Rainbow Bridge is external → Almide (wrap external code as Almide packages). Complementary
- **secure-by-design.md**: design context only; native-code permissions and capability analysis are separate checks, specified in the current package/CLI references above
- **platform-target-separation.md**: `@extern(rs, ...)` corresponds to `platform: native`. Rainbow bridge assumes native platform
- **package-registry.md**: a centralized registry and pre-built native distribution remain separate proposals; current Git-based package resolution does not require that registry

<a id="prerequisites"></a>

## Prerequisites

1. **Current extern/native surface** — exists, but the original `@extern(rs, ...)` sketches here are not a compatibility test.
2. **Package system** — Git/local resolution is implemented; a registry is not a prerequisite for a wrapper package.
3. **Native Cargo integration** — `native/` and `[native-deps]` are implemented and subject to native-code gates.
4. **Automatic bridge/type conversion** — remains proposal work; existing build integration does not supply a general safe ABI conversion generator.

<a id="why-on-hold"></a>

## Why ON HOLD

The remaining proposal is bridge generation and a reviewed ABI/type-conversion
contract, not basic package resolution or Cargo dependency support. Native-code
trust, supported-target behavior and ownership/error conversion need explicit
validation before this plan can claim an ordinary-library safety experience.
