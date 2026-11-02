原文の description メタデータ：@extern を介し外部 Rust/TS/Python コードを native Almide package として包む
<a id="rainbow-bridge--wrap-external-code-as-almide-packages"></a>

# Rainbow Bridge — 外部コードを Almide package として包む

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rainbow-bridge.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](rainbow-bridge.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](rainbow-bridge.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rainbow-bridge.md)

> 原文注記：保留中の自動化提案であり、出荷済み bridge tool や検証済みコード教程ではない。fence は旧構文と未完例（未宣言・import 不足の名前を含む）を保存し、compile していない。型安全 wrapper は motor 選択・状態遷移挙動・外部コードを無誤謬にせず、網羅 match は正しい物理挙動の証明ではない。旧 native manifest と TS fallback は更新された旧概略で、権限や対応 target の案内ではない。ABI 変換、文字列 lifetime/ownership、error にはレビュー済み契約が必要で、native code・依存・build script は信頼対象。既存 package/native 対応は汎用安全 bridge generator を意味しない。


> これは未実装の bridge 自動化設計であり、現行 package/FFI マニュアルではない。コードと `[native]` manifest は当初の概略案。出荷済みの package 解決、native 依存と権限検査には、[package-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md) と [CLI install/deps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md#almide-install) を使う。

<a id="thesis"></a>

## 主張

Rust crate、npm package、Python library などの外部コードを Almide package として包み、利用者には**普通の Almide library に見える**よう提供する。Flutter が iOS/Android native code を plugin に包むのと同じ。

```
Flutter:    Swift/Kotlin native code → Flutter Plugin → import and use
Almide:     Rust/TS/Python/Ruby/etc. → Rainbow Bridge → import and use
```

図の訳：Flutter は Swift/Kotlin native code → Flutter Plugin → import して利用。Almide は Rust/TS/Python/Ruby など → Rainbow Bridge → import して利用。

利用者は `import robot_driver` と書くだけで、何語で実装されたかを知る必要がない。

<a id="why-this-matters"></a>

## なぜ重要か

<a id="there-are-domains-almide-cant-reach-alone"></a>

### Almide 単独では届かない分野がある

- ロボット制御（GPIO、motor、リアルタイム制御）→ Rust crate
- 画像処理 / computer vision（OpenCV など）→ Rust/C++ crate
- ML 推論（ONNX Runtime、TensorFlow Lite など）→ Python library
- 暗号 / 認証（HSM、TPM、OS keychain）→ Rust crate
- database driver（PostgreSQL wire protocol など）→ Rust/TS package
- OS 固有 API（Bluetooth、USB、serial 通信）→ Rust crate
- Web framework（Hono、Express など）→ npm package
- data science（NumPy、pandas など）→ Python library

各言語 ecosystem には既に高品質の実装がある。Almide で再実装するのは無駄。**包むだけで利用できるべきだ。**

<a id="division-of-labor-almide-is-the-safe-layer-external-code-is-the-implementation-layer"></a>

### 分担：「Almide は安全な層、外部コードは実装層」

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

図の訳：Almide 層は状態遷移、業務 logic、データ処理、pattern matching、pipeline、型安全性。Rainbow Bridge（@extern + 型定義）は Rust API を Almide 型で包む。Rust crate 層は hardware 制御、リアルタイム処理、低水準 I/O、性能が重要な処理。

型付き Almide wrapper は native 実装を sandbox 化しない。native source、Cargo 依存と build script は追加の信頼境界であり、native code が build に入る道は `@extern` だけではない。提案 bridge は現行の依存 native 権限と install gate を維持しなければならない。

<a id="package-structure"></a>

## Package 構造

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

構造コメント訳：almide.toml は package 定義と native 依存宣言。src/motor.almd と sensor.almd は @extern で包む Almide API、types.almd は純 Almide の型定義。native/Cargo.toml は Rust crate、native/src/lib.rs は Rust 実装（no_mangle 属性と extern C fn）。

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

設定コメント訳：path は Rust crate の位置。代わりに外部 crate を直接参照してもよい、という原案。

<a id="srctypesalmd--type-definitions-are-pure-almide"></a>

### src/types.almd — 型定義は純 Almide

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

この設計では型定義を Almide に置く。Rust 側の型との自動相互変換は bridge の提案機能であり、型 checker や既存 native 依存統合によって確立されたものではない。

<a id="srcmotoralmd--api-wrapping"></a>

### src/motor.almd — API wrapper

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

コードコメント訳：Rust 実装への bridge。利用者向け API は Almide らしい型安全な interface。後半は純粋な utility 関数。

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

コードコメント訳：JSON 文字列として受け渡す。

<a id="user-experience"></a>

## 利用者体験

利用者コードは Rust の存在を意識しない：

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

**ロボットの状態遷移は Almide の pattern matching で記述し、motor 制御では背後で Rust が動く。利用者は Almide だけ知ればよい。**

<a id="build-integration"></a>

## Build 統合

```bash
almide build app.almd
```

コンパイラの依存解決フロー：

```
1. Read almide.toml
2. robot-driver is a dependency
3. robot-driver/almide.toml has a [native] section
4. Build native/Cargo.toml with cargo build --release
5. Add generated .a / .so to link targets
6. Almide's @extern references native FFI functions
7. Link into final binary
```

フロー訳：1. almide.toml を読む。2. robot-driver が依存にある。3. robot-driver/almide.toml に native section がある。4. cargo build --release で native/Cargo.toml を build。5. 生成 .a / .so を link 対象へ追加。6. Almide の @extern が native FFI 関数を参照。7. 最終 binary に link。

利用者は `almide build` を実行するだけで、`cargo` を知る必要がない。

<a id="extern-rust-side-convention"></a>

## @extern の Rust 側規約

native crate が export する関数は C ABI を使う：

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

コードコメント訳：native/src/lib.rs。hardware 制御実装。sensor_read は JSON 文字列を返し、Almide 側で decode する。

<a id="future-auto-generated-bridge"></a>

### 将来：bridge の自動生成

Rust crate の `pub fn` signature から Almide `@extern` 定義を自動生成：

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

コードコメント訳：src/generated_bridge.almd は自動生成 file。

手動 wrapper なしに Rust crate を Almide から使える。ただし利用者向け型安全 API（MotorId variant → Int 変換など）は手書きする。ここが「Almide らしさ」になる。

<a id="the-almide-touch--what-value-does-wrapping-add"></a>

## Almide らしさ — wrapper が加える価値は何か

Rust crate を直接使わず Almide で包む価値：

**1. 型安全 API**

```rust
// Rust: motor_id is i64. Can't tell if 0 is Left or Right
fn motor_set_speed(motor_id: i64, speed: f64)
```

コードコメント訳：Rust 例では motor_id は i64 で、0 が Left か Right か分からない。

```almide
// Almide: MotorId is a variant. Impossible to get wrong
effect fn set_speed(motor: MotorId, speed: Speed) -> Unit
```

コードコメント訳：Almide 例では MotorId は variant であり、間違えようがない、と原案は述べる。

**2. Pattern matching による状態遷移**

Rust driver は低水準 API だけを提供する。Almide 側で variant + match を使いロボット挙動 logic を書けば、compile 時に状態遷移の網羅を保証する。

**3. Pipeline によるデータ処理**

```almide
let obstacles = read_all_sensors([0, 1, 2, 3])
  |> list.filter((s) => s.confidence > 0.8)
  |> list.filter((s) => s.distance < 2.0)
  |> list.sort_by((s) => s.distance)
```

**4. effect fn による I/O 追跡**

`decide` 関数は pure fn。sensor data を受け取り次の状態を返すだけなので、テストしやすい：

```almide
test "obstacle avoidance" {
  let sensors = [SensorData { distance: 0.2, angle: 0.0, confidence: 0.9, timestamp: 0 }]
  let next = decide(Moving { target: origin, speed: speed_percent(50) }, sensors)
  assert_eq(next, Idle)
}
```

hardware なしで logic をテストでき、effect fn と pure fn の分離がこれを可能にする。

**5. Codec による serialization**

ロボット設定、log、通信 message を Almide の Codec で JSON/binary へ自動変換する：

```almide
let config = json.encode(robot_config)   // Config to JSON
let state = json.decode[RobotState](msg) // Restore state from message
```

コードコメント訳：設定を JSON へ、message から状態を復元。

**6. Web dashboard（当初の target 案、現在は未解決）**

当初の `--target ts` による共有計画は使えず、TypeScript 出力は廃止された。browser interface には、対応する Wasm/host または serialization 境界と別途互換性テストが必要。既存の target 横断型一貫性の保証ではない。

<a id="capability-integration"></a>

## Capability 統合

当初提案の manifest（実装された権限 syntax ではない）：

```toml
# User's almide.toml
[dependencies.robot-driver]
version = "0.1.0"
capabilities = ["hardware"]    # Explicitly grant hardware access
```

設定コメント訳：利用者の almide.toml。hardware access を明示的に許可するという原案。

この `capabilities = ["hardware"]` 案は現行 gate ではない。現在の package 依存 native code は `[permissions].native` で制御し、`almide deps` が報告する。`almide install` は install する root package の native code に別途 `--allow-native` を要求する。capability 解析は `@extern` を保守的に扱い、外部コードの安全性を証明しない。正確な範囲は上記 package/CLI 仕様リンクを参照。

<a id="original-ts-fallback-alternatives-historical-sketches"></a>

## 当初の TS fallback 代案（歴史的概略）

対応する `--target ts` は存在しない。以下は当初の設計議論の記録であり、現行 target 挙動や実行可能な案内ではない：

**案 A：compile error**
```
error: robot-driver requires rainbow bridge (Rust), but target is ts
  = hint: use --target rust, or mock the driver for testing
```

診断案訳：robot-driver は Rainbow Bridge（Rust）を要求するが target は ts。Rust target を使うか、テスト用に driver を mock にする、という hint。

**案 B：mock / stub を提供**
```almide
// src/motor.almd — Write fallback body alongside @extern
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit =
  println("MOCK: set_speed motor=${int.to_string(motor_id)} speed=${float.to_string(speed)}")
```

コードコメント訳：src/motor.almd で @extern と並べて fallback body を書く。

この概略は廃止済み TS target の fallback 挙動を確立しない。対応 target の fallback / mock 戦略は、これらの代案から推測せず実際の extern 実装に対してテストする必要がある。

<a id="relationship-to-other-roadmap-items"></a>

## 他の roadmap 項目との関係

- **rainbow-gate.md**：逆方向。Rainbow FFI Gate は Almide → 外部（Almide を library として公開）。Rainbow Bridge は外部 → Almide（外部コードを Almide package に包む）。相補的。
- **secure-by-design.md**：設計文脈のみ。native-code 権限と capability 解析は別検査で、上記現行 package/CLI 参照に規定。
- **platform-target-separation.md**：`@extern(rs, ...)` は `platform: native` に対応。Rainbow bridge は native platform を前提。
- **package-registry.md**：集中 registry と build 済み native 配布は別の提案のまま。現行 Git ベースの package 解決はその registry を要求しない。

<a id="prerequisites"></a>

## 前提条件

1. **現行 extern/native 表面** — 存在するが、ここにある当初の `@extern(rs, ...)` 概略は互換性テストではない。
2. **Package system** — Git/local 解決は実装済みで、registry は wrapper package の前提ではない。
3. **Native Cargo 統合** — `native/` と `[native-deps]` は実装済みで、native-code gate の対象。
4. **自動 bridge / 型変換** — 提案作業のまま。既存 build 統合は汎用的で安全な ABI 変換 generator を提供しない。

<a id="why-on-hold"></a>

## 保留の理由

残る提案は bridge 生成とレビュー済み ABI/型変換契約であり、基本 package 解決や Cargo 依存対応ではない。普通の library のような安全体験を主張する前に、native-code 信頼、対応 target 挙動、所有権 / error 変換を明示的に検証する必要がある。
