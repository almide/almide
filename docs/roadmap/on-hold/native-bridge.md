# Native Bridge — Rust Crate を Almide パッケージにする [ON HOLD]

## Thesis

Rust クレートを Almide パッケージとしてラップし、ユーザーには **普通の Almide ライブラリにしか見えない** 形で提供する。Flutter が iOS/Android のネイティブコードをプラグインとして包むのと同じ。

```
Flutter:    Swift/Kotlin ネイティブコード → Flutter Plugin → import して使う
Almide:     Rust crate → Native Bridge → import して使う
```

ユーザーは `import robot_driver` と書くだけ。中身が Rust であることを知る必要がない。

## Why This Matters

### Almide 単独では届かない領域がある

- ロボット制御（GPIO、モーター、リアルタイム制御）
- 画像処理 / コンピュータビジョン（OpenCV 等）
- 機械学習推論（ONNX Runtime、TensorFlow Lite 等）
- 暗号・認証（HSM、TPM、OS キーチェーン）
- データベースドライバ（PostgreSQL wire protocol 等）
- OS 固有 API（Bluetooth、USB、シリアル通信）

これらは Rust エコシステムに既に高品質な実装がある。Almide で再実装するのは無駄。**包むだけで使えるべき。**

### 「Almide は安全な層、Rust は低レベル層」の分業

```
┌─────────────────────────────────────────┐
│  Almide                                 │
│  状態遷移、ビジネスロジック、データ処理     │
│  パターンマッチ、パイプライン、型安全      │
├─────────────────────────────────────────┤
│  Native Bridge (@extern + 型定義)        │
│  Almide の型で Rust API をラップ          │
├─────────────────────────────────────────┤
│  Rust crate                             │
│  ハードウェア制御、リアルタイム処理        │
│  低レベルI/O、パフォーマンスクリティカル   │
└─────────────────────────────────────────┘
```

Almide は secure-by-design の安全な世界に閉じたまま。低レベルは Rust に委譲。`@extern` が唯一の境界。

## Package Structure

```
almide-robot-driver/
├── almide.toml                 # パッケージ定義 + native 依存宣言
├── src/
│   ├── motor.almd              # Almide API — @extern でラップ
│   ├── sensor.almd             # Almide API — @extern でラップ
│   └── types.almd              # 型定義 — pure Almide
└── native/
    ├── Cargo.toml              # Rust crate
    └── src/
        └── lib.rs              # Rust 実装 (#[no_mangle] extern "C" fn)
```

### almide.toml

```toml
[package]
name = "robot-driver"
version = "0.1.0"

[native]
type = "rust"
path = "native/"                # Rust crate の場所
# または外部 crate を直接参照
crate = "robot-hal"
version = "0.3"
```

### src/types.almd — 型定義は pure Almide

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

型定義は全部 Almide。パターンマッチ、Codec、型チェックが全部効く。Rust 側の型との変換は bridge 層が自動生成する。

### src/motor.almd — API ラップ

```almide
import types exposing (MotorId, Speed)

// Rust 実装への bridge
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit

@extern(rs, "robot_driver_native", "motor_stop")
effect fn raw_stop(motor_id: Int) -> Unit

@extern(rs, "robot_driver_native", "motor_get_position")
effect fn raw_get_position(motor_id: Int) -> Float

// ---- ユーザーが使う API (Almide らしい型安全なインターフェース) ----

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

// pure な便利関数
fn speed_percent(percent: Int) -> Speed =
  Speed(float.from_int(percent) / 100.0)

fn speed_clamp(speed: Speed, max: Speed) -> Speed = {
  let Speed(v) = speed
  let Speed(m) = max
  Speed(math.min(v, m))
}
```

### src/sensor.almd

```almide
import types exposing (SensorData)

@extern(rs, "robot_driver_native", "sensor_read")
effect fn raw_read(sensor_id: Int) -> String  // JSON 文字列で受け渡し

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

## User Experience

ユーザーコード。Rust の存在を知らない:

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

**ロボットの状態遷移は Almide のパターンマッチで書く。モーター制御は裏で Rust が動く。ユーザーは Almide だけ知ってればいい。**

## Build Integration

```bash
almide build app.almd
```

コンパイラが依存を解決する流れ:

```
1. almide.toml を読む
2. 依存に robot-driver がある
3. robot-driver/almide.toml に [native] セクションがある
4. native/Cargo.toml を cargo build --release でビルド
5. 生成された .a / .so をリンク対象に追加
6. Almide の @extern が native の FFI 関数を参照
7. 最終バイナリにリンク
```

ユーザーは `almide build` だけ。`cargo` を知らなくていい。

## @extern の Rust 側規約

Native crate が公開する関数は C ABI:

```rust
// native/src/lib.rs

#[no_mangle]
pub extern "C" fn motor_set_speed(motor_id: i64, speed: f64) {
    // ハードウェア制御の実装
}

#[no_mangle]
pub extern "C" fn motor_stop(motor_id: i64) {
    // ...
}

#[no_mangle]
pub extern "C" fn sensor_read(sensor_id: i64) -> *const c_char {
    // JSON 文字列を返す (Almide 側で decode)
}
```

### 将来: 自動 bridge 生成

Rust crate の `pub fn` シグネチャから Almide の `@extern` 定義を自動生成する:

```bash
almide bridge generate --from native/Cargo.toml --out src/generated_bridge.almd
```

```almide
// src/generated_bridge.almd (自動生成)
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn motor_set_speed(motor_id: Int, speed: Float) -> Unit

@extern(rs, "robot_driver_native", "motor_stop")
effect fn motor_stop(motor_id: Int) -> Unit
```

手動ラッパーなしで Rust crate が Almide から使える。ただしユーザー向けの型安全な API（MotorId variant → Int 変換等）は手で書く。これが「Almide らしさ」になる。

## Almide らしさ — 何が付加価値か

Rust crate をそのまま使うのではなく、Almide でラップすることの価値:

**1. 型安全な API**

```rust
// Rust: motor_id は i64。0 が Left か Right か分からない
fn motor_set_speed(motor_id: i64, speed: f64)
```

```almide
// Almide: MotorId は variant。間違えようがない
effect fn set_speed(motor: MotorId, speed: Speed) -> Unit
```

**2. パターンマッチで状態遷移**

Rust のドライバは低レベル API を提供するだけ。Almide 側でロボットの行動ロジックを variant + match で書くと、状態遷移の網羅性がコンパイル時に保証される。

**3. パイプラインでデータ処理**

```almide
let obstacles = read_all_sensors([0, 1, 2, 3])
  |> list.filter((s) => s.confidence > 0.8)
  |> list.filter((s) => s.distance < 2.0)
  |> list.sort_by((s) => s.distance)
```

**4. effect fn で I/O 追跡**

`decide` 関数は pure fn。センサーデータを受け取って次の状態を返すだけ。テストが簡単:

```almide
test "obstacle avoidance" {
  let sensors = [SensorData { distance: 0.2, angle: 0.0, confidence: 0.9, timestamp: 0 }]
  let next = decide(Moving { target: origin, speed: speed_percent(50) }, sensors)
  assert_eq(next, Idle)
}
```

ハードウェアなしでロジックをテストできる。effect fn と pure fn の分離がこれを可能にする。

**5. Codec でシリアライズ**

ロボットの設定、ログ、通信メッセージが Almide の Codec で JSON/バイナリに自動変換:

```almide
let config = json.encode(robot_config)   // 設定を JSON に
let state = json.decode[RobotState](msg) // メッセージから状態を復元
```

**6. --target ts で Web ダッシュボード**

同じ型定義（RobotState, SensorData）を `--target ts` で Web UI にも使える。サーバーサイドは Rust native、ダッシュボードは TS。型が一致してることがコンパイル時に保証される。

## Capability Integration

secure-by-design (→ secure-by-design.md) との統合:

```toml
# ユーザーの almide.toml
[dependencies.robot-driver]
version = "0.1.0"
capabilities = ["hardware"]    # ハードウェアアクセスを明示的に許可
```

native bridge を持つパッケージは自動的に capability が推論される。`@extern(rs, ...)` があるので `hardware` (または `native`) capability が必要。ユーザーが明示的に許可しないと使えない。

## --target ts のときどうなるか

Native bridge は `--target rust` でしか動かない。`--target ts` でビルドすると:

**Option A: コンパイルエラー**
```
error: robot-driver requires native bridge (Rust), but target is ts
  = hint: use --target rust, or mock the driver for testing
```

**Option B: mock / stub を提供**
```almide
// src/motor.almd — @extern の横に fallback body を書く
@extern(rs, "robot_driver_native", "motor_set_speed")
effect fn raw_set_speed(motor_id: Int, speed: Float) -> Unit =
  println("MOCK: set_speed motor=${int.to_string(motor_id)} speed=${float.to_string(speed)}")
```

`@extern` の fallback body は既に言語仕様にある。`--target ts` では body が使われ、`--target rust` では native が使われる。テスト用のモックが自然に書ける。

## Relationship to Other Roadmap Items

- **rainbow-ffi.md**: 方向が逆。rainbow-ffi は Almide → 外 (Almide をライブラリとして公開)。native-bridge は 外 → Almide (Rust を Almide パッケージにする)。補完関係
- **secure-by-design.md**: native bridge パッケージは capability `native` が必要。capability 推論で自動検出される
- **platform-target-separation.md**: `@extern(rs, ...)` は `platform: native` に相当。native bridge は native platform 前提
- **package-registry.md**: native bridge パッケージの配布にはレジストリが必要。native 部分のプリビルドバイナリ配布も検討

## Prerequisites

1. **`@extern(rs, ...)` fallback body** — ✅ 既に動いてる
2. **パッケージシステム** — ❌ 基本的なパッケージ解決が必要
3. **`almide build` の Cargo 統合** — ❌ native/ の自動ビルド
4. **C ABI の型変換** — ❌ Almide 型 ↔ C 型の自動変換

## Why ON HOLD

パッケージシステムとビルド統合が前提。ただし:

- **`@extern` は既に動いてる** — 手動で Rust crate をビルドして Almide から参照することは今でもできる
- **fallback body も動いてる** — テスト時のモックが書ける
- **型定義は pure Almide** — native bridge がなくても型とロジックは書ける

自動化 (almide build の Cargo 統合、bridge 自動生成) がないだけで、手動では今日でもできる。
