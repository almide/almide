原文 description 元数据：通过 @extern 将外部 Rust/TS/Python 代码包装为 native Almide package
<a id="rainbow-bridge--wrap-external-code-as-almide-packages"></a>

# Rainbow Bridge — 将外部代码包装为 Almide package

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rainbow-bridge.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](rainbow-bridge.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](rainbow-bridge.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rainbow-bridge.md)

> 原文注记：这是暂缓自动化提案，不是已交付 bridge 工具或已验证代码教程。fence 保留旧语法及不完整示例（含未声明/未导入名称），未编译。类型安全 wrapper 不使 motor 选择、状态行为或外部代码绝无错误；穷尽 match 不是正确物理行为的证明。旧 native manifest 与 TS fallback 是已被更新的历史草图，不是权限或受支持 target 指南。ABI 转换、字符串 lifetime/ownership、error 须经审查的契约；native code、依赖、build script 仍受信任。已有 package/native 支持不意味着通用安全 bridge generator。


> 这是尚未实现的 bridge 自动化设计，不是当前 package/FFI 手册。代码与 `[native]` manifest 是原始草图。已交付的 package 解析、native 依赖及权限检查，请参阅 [package-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md) 和 [CLI install/deps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md#almide-install)。

<a id="thesis"></a>

## 主张

将 Rust crate、npm package、Python library 等外部代码包装为 Almide package，让用户觉得它们**像普通 Almide library**。类似 Flutter 将 iOS/Android native code 包装为 plugin。

```
Flutter:    Swift/Kotlin native code → Flutter Plugin → import and use
Almide:     Rust/TS/Python/Ruby/etc. → Rainbow Bridge → import and use
```

图示译文：Flutter：Swift/Kotlin native code → Flutter Plugin → import 使用。Almide：Rust/TS/Python/Ruby 等 → Rainbow Bridge → import 使用。

用户只需写 `import robot_driver`，不必知道它用哪种语言编写。

<a id="why-this-matters"></a>

## 为什么重要

<a id="there-are-domains-almide-cant-reach-alone"></a>

### 有些领域 Almide 无法独自触及

- 机器人控制（GPIO、motor、实时控制）→ Rust crate
- 图像处理 / computer vision（OpenCV 等）→ Rust/C++ crate
- ML 推理（ONNX Runtime、TensorFlow Lite 等）→ Python library
- 密码学 / 身份验证（HSM、TPM、OS keychain）→ Rust crate
- database driver（PostgreSQL wire protocol 等）→ Rust/TS package
- OS 专属 API（Bluetooth、USB、串行通信）→ Rust crate
- Web framework（Hono、Express 等）→ npm package
- data science（NumPy、pandas 等）→ Python library

各语言生态已有高质量实现。在 Almide 重写是浪费。**应该只需包装就能使用。**

<a id="division-of-labor-almide-is-the-safe-layer-external-code-is-the-implementation-layer"></a>

### 分工：“Almide 是安全层，外部代码是实现层”

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

图示译文：Almide 层负责状态转换、业务逻辑、数据处理、模式匹配、流水线、类型安全。Rainbow Bridge（@extern + 类型定义）用 Almide 类型包装 Rust API。Rust crate 层负责硬件控制、实时处理、低层 I/O、性能关键工作。

类型化 Almide wrapper 不会将 native 实现 sandbox 化。native source、Cargo 依赖及 build script 是额外信任边界；`@extern` 并非 native code 进入 build 的唯一途径。提议 bridge 必须保留当前依赖 native 权限和 install 门禁。

<a id="package-structure"></a>

## Package 结构

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

结构注释译文：almide.toml 为 package 定义及 native 依赖声明；src/motor.almd 和 sensor.almd 是通过 @extern 包装的 Almide API；types.almd 是纯 Almide 类型定义。native/Cargo.toml 是 Rust crate，native/src/lib.rs 是 Rust 实现（no_mangle 属性、extern C fn）。

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

配置注释译文：path 是 Rust crate 的位置；也可直接引用外部 crate，这是原提案。

<a id="srctypesalmd--type-definitions-are-pure-almide"></a>

### src/types.almd — 类型定义为纯 Almide

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

设计将类型定义保留在 Almide 中。与 Rust 类型自动互转是提议 bridge 功能，并非类型 checker 或现有 native 依赖集成已经确立的能力。

<a id="srcmotoralmd--api-wrapping"></a>

### src/motor.almd — API 包装

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

代码注释译文：连接 Rust 实现的 bridge；面向用户的 API 是符合 Almide 风格的类型安全 interface；后半为纯 utility 函数。

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

代码注释译文：作为 JSON 字符串传递。

<a id="user-experience"></a>

## 用户体验

用户代码不感知 Rust 存在：

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

**机器人状态转换用 Almide 模式匹配编写，motor 控制在背后运行 Rust。用户只需了解 Almide。**

<a id="build-integration"></a>

## Build 集成

```bash
almide build app.almd
```

编译器依赖解析流程：

```
1. Read almide.toml
2. robot-driver is a dependency
3. robot-driver/almide.toml has a [native] section
4. Build native/Cargo.toml with cargo build --release
5. Add generated .a / .so to link targets
6. Almide's @extern references native FFI functions
7. Link into final binary
```

流程译文：1. 读取 almide.toml。2. robot-driver 是依赖。3. robot-driver/almide.toml 有 native section。4. 用 cargo build --release 构建 native/Cargo.toml。5. 将生成的 .a / .so 加入 link target。6. Almide @extern 引用 native FFI 函数。7. 链接进最终 binary。

用户只运行 `almide build`，无需了解 `cargo`。

<a id="extern-rust-side-convention"></a>

## @extern 的 Rust 侧约定

native crate 导出的函数使用 C ABI：

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

代码注释译文：native/src/lib.rs；硬件控制实现；sensor_read 返回 JSON 字符串（Almide 侧 decode）。

<a id="future-auto-generated-bridge"></a>

### 未来：自动生成 bridge

从 Rust crate `pub fn` signature 自动生成 Almide `@extern` 定义：

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

代码注释译文：src/generated_bridge.almd 为自动生成文件。

无需手写 wrapper 即可从 Almide 使用 Rust crate。但面向用户的类型安全 API（MotorId variant → Int 转换等）手工编写，赋予其“Almide 风格”。

<a id="the-almide-touch--what-value-does-wrapping-add"></a>

## Almide 风格 — 包装增加什么价值

相对直接使用 Rust crate，以 Almide 包装的价值：

**1. 类型安全 API**

```rust
// Rust: motor_id is i64. Can't tell if 0 is Left or Right
fn motor_set_speed(motor_id: i64, speed: f64)
```

代码注释译文：Rust 示例中 motor_id 为 i64，无法判断 0 是 Left 还是 Right。

```almide
// Almide: MotorId is a variant. Impossible to get wrong
effect fn set_speed(motor: MotorId, speed: Speed) -> Unit
```

代码注释译文：Almide 示例中 MotorId 是 variant；原提案称“不可能弄错”。

**2. 通过模式匹配进行状态转换**

Rust driver 仅提供低层 API。在 Almide 侧用 variant + match 编写机器人行为逻辑，可在编译时保证穷尽覆盖状态转换。

**3. 通过流水线处理数据**

```almide
let obstacles = read_all_sensors([0, 1, 2, 3])
  |> list.filter((s) => s.confidence > 0.8)
  |> list.filter((s) => s.distance < 2.0)
  |> list.sort_by((s) => s.distance)
```

**4. 通过 effect fn 跟踪 I/O**

`decide` 是 pure fn，仅接收 sensor data、返回下一状态，易于测试：

```almide
test "obstacle avoidance" {
  let sensors = [SensorData { distance: 0.2, angle: 0.0, confidence: 0.9, timestamp: 0 }]
  let next = decide(Moving { target: origin, speed: speed_percent(50) }, sensors)
  assert_eq(next, Idle)
}
```

无需硬件便可测试逻辑，effect fn 与 pure fn 分离使之成为可能。

**5. 通过 Codec 序列化**

机器人配置、log、通信 message 通过 Almide Codec 自动转换为 JSON/binary：

```almide
let config = json.encode(robot_config)   // Config to JSON
let state = json.decode[RobotState](msg) // Restore state from message
```

代码注释译文：配置转为 JSON；从 message 恢复状态。

**6. Web dashboard（原 target 想法，目前未解决）**

原 `--target ts` 共享计划不可用：TypeScript 输出已退役。浏览器 interface 需要受支持的 Wasm/host 或序列化边界，并单独测试兼容性；它不是现有跨 target 类型一致性保证。

<a id="capability-integration"></a>

## Capability 集成

原提议 manifest（并非已实现权限语法）：

```toml
# User's almide.toml
[dependencies.robot-driver]
version = "0.1.0"
capabilities = ["hardware"]    # Explicitly grant hardware access
```

配置注释译文：用户的 almide.toml；原案显式授予 hardware access。

`capabilities = ["hardware"]` 草图不是当前门禁。当前 package 依赖 native code 由 `[permissions].native` 控制；`almide deps` 报告它，`almide install` 对被安装 root package 的 native code 另要求 `--allow-native`。Capability 分析保守处理 `@extern`，并不证明外部代码安全。精确范围见上方 package/CLI 规范链接。

<a id="original-ts-fallback-alternatives-historical-sketches"></a>

## 原 TS fallback 方案（历史草图）

不存在受支持的 `--target ts`。下述方案记录原设计讨论，不是当前 target 行为或可运行指南：

**方案 A：编译错误**
```
error: robot-driver requires rainbow bridge (Rust), but target is ts
  = hint: use --target rust, or mock the driver for testing
```

诊断草图译文：robot-driver 要求 Rainbow Bridge（Rust），但 target 为 ts；提示使用 Rust target 或为测试 mock driver。

**方案 B：提供 mock / stub**
```almide
// src/motor.almd — Write fallback body alongside @extern
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit =
  println("MOCK: set_speed motor=${int.to_string(motor_id)} speed=${float.to_string(speed)}")
```

代码注释译文：在 src/motor.almd 中随 @extern 一起写 fallback body。

此草图不确立已退役 TS target 的 fallback 行为。受支持 target 的 fallback / mock 策略必须针对实际 extern 实现测试，不应从这些替代方案推断。

<a id="relationship-to-other-roadmap-items"></a>

## 与其他 roadmap 项目的关系

- **rainbow-gate.md**：相反方向。Rainbow FFI Gate 为 Almide → 外部（将 Almide 作为 library 发布）；Rainbow Bridge 为外部 → Almide（把外部代码包装成 Almide package）。二者互补。
- **secure-by-design.md**：仅设计背景；native-code 权限和 capability 分析是独立检查，由上述当前 package/CLI 参考规定。
- **platform-target-separation.md**：`@extern(rs, ...)` 对应 `platform: native`；Rainbow bridge 假设 native platform。
- **package-registry.md**：中心 registry 和预构建 native 分发仍为独立提案；当前基于 Git 的 package 解析不需要该 registry。

<a id="prerequisites"></a>

## 前提

1. **当前 extern/native 表面** — 已存在，但这里原始 `@extern(rs, ...)` 草图并非兼容性测试。
2. **Package system** — Git/local 解析已实现；registry 不是 wrapper package 的前提。
3. **Native Cargo 集成** — `native/` 与 `[native-deps]` 已实现，受 native-code 门禁控制。
4. **自动 bridge/类型转换** — 仍是提议工作；现有 build 集成不提供通用安全 ABI 转换 generator。

<a id="why-on-hold"></a>

## 暂缓原因

剩余提案是 bridge 生成和经过审查的 ABI/类型转换契约，不是基础 package 解析或 Cargo 依赖支持。计划若要宣称普通 library 般的安全体验，须先显式验证 native-code 信任、支持 target 行为和 ownership/error 转换。
