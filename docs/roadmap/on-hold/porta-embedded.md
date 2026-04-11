<!-- description: Porta-style WASI agent runtime for IoT: <10KB Almide guests on tiny hosts -->
# Porta Embedded — Sub-10KB Almide IoT Agents on WASI Hosts

**Status:** on-hold / draft memo
**Priority:** after porta v1 and graphics stack stabilizes
**Prerequisites:** WASI Preview 2 sockets binding in Almide stdlib, wasi-tls proposal landing, size-optimized WASM emit pass in Almide codegen

このドキュメントは「Almide なら 10 KB 全部載せの IoT OS いけるんじゃないか」という問いに対する戦略メモ。結論は **「bare-metal RTOS クローンは無理、でも WASI ゲスト + tiny host の分業なら 10 KB IoT agent は現実的」**。porta の philosophy を embedded 領域に延ばす形。

---

## 1. 背景 — なぜ bare-metal RTOS クローンを追わないか

TinyOS 風「10 KB 全部載せ RTOS」記事への技術的 reality check から出発した議論。

### C 実装で膨らむ原因と、Almide で削れる幅

| C で膨らむ理由 | Almide での削減可能性 |
|---|---|
| libc + crt0 + printf 連鎖 | **ゼロ**。Almide は libc を持たない (obsid 3.8 KB が実証) |
| 各 `.c` 独立コンパイル、LTO 不完全 | whole-program compilation + 関数単位 DCE がデフォルト |
| mbedTLS 200+ 関数 export | monomorphization で ciphersuite 1 個だけ特殊化 |
| `#ifdef` による coarse-grained 設定 | compile-time const propagation で細粒度に剥がせる |
| vtable / dyn dispatch | effect fn は Result への単純書き換え、vtable なし |
| WASM vs Thumb-2 命令サイズ | **逆風**: WASM bytecode は Thumb-2 より 1.5〜2x 大きい |

compile 技術的優位は本物で、**obsid 3.8 KB = C+WebGL 同等機能の 5〜8 倍 density** という前例が裏付けている。

### 削れない部分 (物理/数学)

| 機能 | 最小実装の irreducible サイズ | 備考 |
|---|---|---|
| AES-128-GCM (soft) | ~1.5 KB (S-box + GF mul) | 言語変えても不変 |
| SHA-256 (soft) | ~800 B | 同 |
| X25519 (big integer mul) | ~2 KB | 同 |
| X.509 DER parser | ~5 KB (ASN.1 + OID table) | mono で 1〜2 KB まで圧縮可能 |
| TCP 状態遷移 | ~5 KB | 2〜3 KB まで圧縮可能 |
| MQTT QoS 2 | ~3 KB | 1.5〜2 KB まで圧縮可能 |

**ソフト実装下限で ~20〜25 KB**。10 KB には届かない。crypto 数学部分は言語を変えても圧縮できない。

---

## 2. 10 KB に届く 3 つの手段

### 手段 A — Hardware offload で数学部分を消す

現代 MCU (Cortex-M33, ESP32-S3, RP2350, STM32U5 等) の crypto ハードウェアを使う:
- ARMv8 Crypto Extensions (AES/SHA H/W)
- STM32 crypto peripheral
- ESP32 HMAC / SHA / AES accelerator
- PKA (Public Key Accelerator) — ECDHE / RSA H/W

Almide TLS library は **thin FFI shim** になる:

```almide
@extern(mmio, "crypto", "aes_gcm_encrypt")
fn aes_gcm_encrypt(key: Bytes, iv: Bytes, data: Bytes) -> Bytes
```

crypto 15 KB が消えて ~500 B になり、残り 9.5 KB で MQTT + TCP glue + scheduler を書く余地が生まれる。**10 KB 全部載せが数値的に現実味を帯びる**。

**弱点:** 特定 MCU 依存が強い、ポータビリティが落ちる。

### 手段 B — 「10 KB」の定義を変える: WASI ゲスト (本命)

Almide は **WASI-compliant tiny host** の上で動くゲスト。TLS / TCP / scheduler は host runtime が提供、Almide はアプリケーション層だけ書く。

```
┌─────────────────────────────────┐
│ Almide app (< 10 KB)            │ ← MQTT publish ロジック、業務ルール
├─────────────────────────────────┤
│ Almide stdlib (0〜4 KB)         │ ← bytes, string, math のみ
├─────────────────────────────────┤
│ WASI Preview 2 imports          │ ← sockets, clocks, random, tls (wasi-tls)
├─────────────────────────────────┤
│ Tiny WASI host (別バイナリ)      │ ← wasm-micro-runtime / wasm3 + lwIP + mbedTLS
└─────────────────────────────────┘
```

- **Almide 部分は 10 KB 以内が現実的** (obsid 3.8 KB の路線をそのまま踏襲)
- TLS / TCP / scheduler は host runtime 側の責務
- Almide の sell point: **「同じ 10 KB のゲストが、WASI 対応ホストならどこでも動く」** — cross-host moat

**これは porta の architecture を embedded に展開したもの**。
- porta (既存): macOS / Linux host + wasmtime で AI agent を sandbox 実行
- porta embedded: ESP32 / STM32 host + wasm-micro-runtime で IoT agent を sandbox 実行
- 両者とも **capability 管理 + WASI guest execution** がコア

既存の tiny WASI runtime:
| runtime | 典型サイズ | 備考 |
|---|---|---|
| wasm3 | ~64 KB | インタプリタ、最小クラス |
| wasm-micro-runtime (WAMR) | ~85 KB〜 | AOT/Interp 両対応、Bytecode Alliance 公式 |
| wasmtime | ~10 MB | JIT、desktop 前提 |
| 自作 tiny runtime | — | 可能だが投資必要 |

WAMR / wasm3 を borrow すれば host 側 ~64〜85 KB、Almide ゲスト側 < 10 KB という分担になる。

### 手段 C — Almide 自身が runtime も書く

Almide で tiny WASI runtime を実装する。つまり Almide 自身が host の役割も担う。

- bare-metal access、割込み、asm、linker section 制御が必要
- **現状の Almide compiler には無い** (ml-inference.md と同じ制約)
- 6〜12 ヶ月の compiler 側投資が必要

**判定:** 長期的には興味深いが、今は除外。手段 B を先にやる。

---

## 3. 推奨路線 — 手段 B (porta embedded)

### なぜ B か

- Almide の 5 軸 (tiny binary / LLM authoring / zero-copy / gamma-correct / cross-host) が全部効く
- porta の philosophy を embedded に展開、strategic bet が連動
- compiler 側の新規投資が最小 (WASI Preview 2 binding の追加程度)
- 実機デモまでの距離が最短 (WAMR / wasm3 既存 runtime を借りれば最短ルート)
- 「10 KB IoT agent + tiny host 合計 100 KB 未満」という数字が sell possible

### 兄弟プロジェクトとしての関係

```
porta (native host, wasmtime)     ←→  porta embedded (MCU host, WAMR/wasm3)
  │                                       │
  └──── capability model 共有 ─────────────┘
  └──── manifest.json 共有 ────────────────┘
  └──── MCP 契約 (option) ─────────────────┘
```

`porta.toml` の書き方も統一できれば、**開発者は desktop porta で prototype → そのまま embedded porta に deploy** というワークフローが成立する。これは porta 単体では実現できない差別化。

---

## 4. 段階的ロードマップ

### Phase 0 — WASI Preview 2 サポート (2〜3 週間)

- Almide stdlib に `wasi-sockets-tcp` binding を追加
- Almide stdlib に `wasi-clocks`, `wasi-random` binding を追加
- wasmtime (desktop) 上で簡単な TCP echo client を Almide で書いて動かす
- **成果物:** desktop wasmtime で動く minimum TCP client

### Phase 1 — MQTT publish agent PoC (3〜4 週間)

- Almide で MQTT 3.1.1 QoS 0 publisher を書く (TLS なし、平文 TCP)
- 目標: Almide + stdlib で **< 8 KB WASM**
- broker.example.com への publish を実機テスト
- **成果物:** `agent-mqtt-pub.wasm`、ベンチ数字公開

### Phase 2 — TLS 統合 (wasi-tls 依存) (未定)

- `wasi-tls` proposal が landing したら Almide stdlib binding 追加
- MQTT over TLS publisher を書く
- 目標: Almide 側 **< 10 KB WASM**、host 側 TLS は wasi-tls 実装に委譲
- **成果物:** `agent-mqtt-tls-pub.wasm`

### Phase 3 — tiny host 選定と MCU 実機 (1〜2 ヶ月)

- WAMR vs wasm3 ベンチ (size, perf, WASI Preview 2 coverage)
- ESP32-S3 / STM32U5 / RP2350 のいずれかで実機動作
- メッセージ rate、電力、メモリ使用量測定
- **成果物:** MCU 実機デモ、ブログ記事用の数字

### Phase 4 — porta embedded 公開 (2〜3 ヶ月)

- `porta-embedded` リポとして切り出し (or porta repo 内サブディレクトリ)
- 共通 manifest.json フォーマット、共通 capability model
- ドキュメント整備、複数の example agent
- **成果物:** `almide/porta-embedded` v0.1.0

### Phase 5 — MCU ベンダー別 HAL ラッパ (継続)

- ESP-IDF, STM32Cube, nRF Connect SDK 向けの thin adapter
- host 側の WAMR 統合テンプレート
- developer experience を Desktop porta に揃える

---

## 5. スコープアウト

- **Bare-metal から自作 RTOS** — 手段 C、長期保留
- **Soft TLS 実装を Almide で書く** — crypto 数学は irreducible、やる意味なし
- **カスタム tiny WASI runtime** — WAMR/wasm3 を借りる、自作は最後の手段
- **Training / ML** — ml-inference.md で独立 track
- **Graphics** — obsid / canvas2d で独立 track
- **Ethernet / Wi-Fi driver** — host 側の責務、Almide 側では触らない

---

## 6. 差別化軸

graphics stack / ML inference と共通の 5 軸で再評価:

| 軸 | porta embedded の強み |
|---|---|
| Binary size | < 10 KB ゲスト、host 込み 100 KB 以下 |
| LLM authoring | IoT 業務ロジックを LLM に書かせやすい (C より) |
| Zero-copy | WASI linear memory 経由、serialization なし |
| Cross-host | WASI Preview 2 対応 host ならどこでも動く |
| Capability security | porta の capability model が embedded にも届く |

graphics / ML と同じく、**Almide の核心思想が embedded にもシームレスに延びる**ことが狙い。

---

## 7. Porta (既存) との関係

| 項目 | porta (既存) | porta embedded |
|---|---|---|
| ターゲット | Desktop / Server | MCU / IoT device |
| Host runtime | wasmtime (~10 MB) | WAMR / wasm3 (~64〜85 KB) |
| Use case | AI agent sandbox、Claude Code 制御 | IoT agent、sensor/actuator 制御 |
| OS layer | macOS / Linux + sandbox-exec | bare metal (RTOS なし or 薄い RTOS) |
| Capability | FS / Network / Exec | GPIO / I2C / SPI / Network |
| Manifest | porta.toml + manifest.json | 同じ (できれば完全共通) |

**「同じ Almide agent コードが、Desktop でも MCU でも動く」** を合言葉にする。これは他の embedded runtime (Rust Embassy, Zephyr, FreeRTOS) では実現できない差別化。

---

## 8. 保留理由と再開条件

### 今やらない理由

- porta (既存) がまだ v0.1、v1 凍結前
- graphics stack strategy (obsid/docs/strategy.md) が未確定
- WASI Preview 2 sockets / wasi-tls proposal が standardize 途中
- Almide stdlib に WASI Preview 2 binding が無い

### 再開条件

1. porta v1.0 が public release される
2. WASI Preview 2 sockets が stable になる
3. Almide stdlib に Preview 2 binding が追加される
4. graphics stack で少なくとも 1 パッケージが v1.0 凍結される (Almide compiler の stabilization signal)

これらが揃ったら `active/` に移動、Phase 0 着手。

---

## 9. 関連 roadmap

- `on-hold/ml-inference.md` — 同じ Almide 差別化軸で ML inference runtime を追う、WASI guest + tiny host の構図が共通
- `obsid/docs/strategy.md` (外部リポ) — graphics stack の strategy、cross-host moat の議論が共通
- `active/`(?) — wasm-simd128 intrinsics があれば crypto soft fallback の余地も広がる

**強い connection:** 手段 B (WASI guest) の architecture は graphics stack strategy doc の C2 (abstract host interface) と **完全に同一の設計原則**。porta embedded / ml-inference / graphics の 3 track が全部同じ backend pluggability 思想で動くなら、Almide は **「WASI 上の tiny WASM agent / renderer / inference」** という統一された strategic position を取れる。
