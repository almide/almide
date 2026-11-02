原文の description メタデータ：組込み WASI agent runtime の提案。サイズと cross-host 目標には hardware 上の証拠が必要
<a id="porta-embedded--sub-10kb-almide-iot-agents-on-wasi-hosts"></a>

# Porta Embedded — WASI host 上の 10KB 未満の Almide IoT agent

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](porta-embedded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](porta-embedded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](porta-embedded.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/porta-embedded.md)

> 原文注記：保留中の設計であり、配備済み MCU runtime や独立測定したサイズ・安全性の結果ではない。原文の日付付き外部確認は今回の翻訳で再実施していない。当初 desktop 比較と Porta native process に関する明示的訂正を併せて保持する。runtime 候補は配備先の選定ではなく、schema や WASI 名称の共有は同等の強制力を示さない。Phase 0 の binding が再開前提にも現れる計画上の依存は、ここでは未解決のまま。


**優先度：** `porta` が v1.0 に達し graphics stack が安定した後
**前提条件：** Almide stdlib の WASI Preview 2 sockets binding、`wasi-tls` 提案の導入、サイズ最適化 WASM emit pass
**設計目標：** 同じ必要 interface と意味論を実装する desktop と MCU の host 間で Almide agent を再利用する。「WASI 準拠」だけで互換性が成立するわけではない。
**差別化案：** 選択した desktop/MCU host 間で guest を変更せず配備する。ここでは実証しておらず、市場全体での独占的優位も立証していない。

> **未測定の目標であり、結果ではない：** guest ≤ 10 KB、host + guest ≤ 110 KB。
> 合計には選んだ TLS/network stack、asset、platform 接続部分を含めなければならない。
> flash サイズと peak RAM は別の測定である。

<a id="external-baseline-checked-on-2026-10-31"></a>

## 2026-10-31 に確認した外部基準

[Porta の `123bef31`](https://github.com/almide/porta/blob/123bef319c99610dd94fb73c28f6443c5a03fc73/README.md) は native process sandbox（macOS Seatbelt、Linux Landlock/seccomp/namespaces）と、別の Wasm mode を説明する。desktop Wasmtime host だけではない。どちらの mode も MCU への拡張には作業が必要であり、安全性の保証を引き継げるわけではない。

以下の Obsid 3.8 KB は [2026-04-11 の戦略 draft](https://github.com/almide-graphics/obsid/blob/7e31406ee983f43d21cb95ddcb63861bed6c6648/docs/strategy.md) の値であり、薄い JS bridge を使う browser WebGL engine を説明する。歴史的な値であって、完全な native/embedded graphics runtime の測定ではない。この review では外部 project の build や hardware 測定を行っていない。再開条件は自動的に達成されたのではなく、未達成／未確認のままである。

---

<a id="thesis"></a>

## 主張

Almide には 10 KB IoT agent への現実的な道がある。ただし自ら RTOS に*なろう*とせず、TLS、TCP/IP、scheduling、crypto を import として提供する WASI 準拠 host の*内部で動く*場合に限る。この提案では TLS cipher、certificate parse、TCP state を host に委ねる。当初の engine 見積り 64–85 KB は、これらの service を含む host のサイズを示さない。Almide が担うのは application layer、MQTT framing、business rule、capability policy であり、言語の LLM による記述や compile-time 特殊化が意味を持つ部分である。

これは `porta` の哲学を MCU へ拡張するもの：*capability で制御する WASM guest、service を提供する host、移植可能な agent binary。*

---

<a id="why-not-a-bare-metal-rtos-clone"></a>

## なぜ bare-metal RTOS clone ではないのか

以下の当初計画の見積りが system service 委譲の動機になった。数学的下限ではない。code size は feature subset、hardware offload、toolchain、測定に何を含めるかで変わる。

| 当初見積りの subsystem | 参考サイズ（再測定なし）| 根拠 |
|---|---|---|
| AES-128-GCM（software）| ~1.5 KB | S-box + Galois 体乗算 |
| SHA-256（software）| ~0.8 KB | round 定数 + mixing 関数 |
| X25519 | ~2 KB | 多倍長整数算術 |
| X.509 DER parser | ~5 KB | ASN.1 decoder + OID table |
| TCP state machine | ~5 KB | 再送 + 輻輳 + sequence |
| MQTT 3.1.1 QoS 2 | ~3 KB | 4-way handshake の状態追跡 |

当初の ~20–25 KB は、証明済みの software-only 下限ではない。検討する選択肢：

1. crypto と TCP の **hardware offload**（STM32 CryptoCell、ESP32 hardware AES、NXP MAC+IP）。可能だが、agent が特定 MCU 系列に結びつく。
2. 重い stack を既に実装する **WASI host への委譲**。選んだ host 上で移植性と同等の強制力を実証する必要がある。

この roadmap が採るのは経路 2 である。

---

<a id="where-almides-size-advantage-comes-from"></a>

## Almide のサイズ上の優位はどこから来るのか

歴史的な Obsid/browser の値は、旧来の Rust 比 5×–8× という比較や組込みへの適用を立証しない。実際の guest 上で測定する候補の仕組みは：

- **libc なし。** Almide の WASM emit は crt0、printf、format parse の仕組みを取り込まない。
- **whole-program DCE。** 関数単位の dead-code elimination が既定で動き、呼ばれる*かもしれない*というだけでは binary に何も残らない。
- **単相化。** Almide で一度書いた TLS library が、使用する ciphersuite だけに特殊化される。代替経路は compile-time に消える。
- **compile-time 定数伝播。** C の `#ifdef` よりはるかに細かく、設定選択を直接置換する。
- **vtable 層なし。** `effect fn` は単純な Result early-return に書き換わり、dynamic dispatch cost がない。

Wasm/Thumb-2 の密度や最適化の利益には、同一 program・同一 feature の測定が必要。固定比率や必ず相殺できるという保証はここでは立証していない。

---

<a id="architecture"></a>

## アーキテクチャ

```
┌──────────────────────────────────────────┐
│  Almide IoT agent (< 10 KB WASM)          │  business logic, MQTT framing
├──────────────────────────────────────────┤
│  Almide stdlib (0–4 KB)                   │  bytes, string, math
├──────────────────────────────────────────┤
│  WASI Preview 2 imports                   │  sockets, clocks, random, tls
├──────────────────────────────────────────┤
│  Tiny WASI host (separate binary)         │  WAMR / wasm3 + lwIP + mbedTLS
├──────────────────────────────────────────┤
│  MCU hardware                             │  ESP32-S3 / STM32U5 / RP2350
└──────────────────────────────────────────┘
```

図の訳：Almide IoT agent（< 10 KB WASM、business logic と MQTT framing）／Almide stdlib（0–4 KB、bytes・string・math）／WASI Preview 2 imports（sockets・clocks・random・tls）／小型 WASI host（別 binary、WAMR / wasm3 + lwIP + mbedTLS）／MCU hardware（ESP32-S3 / STM32U5 / RP2350）。

これは interface 配置の提案であり、現行 binding を動かす手順ではない。同じ artifact が両 host で動くと主張する前に、Preview 2/component ABI 対応、adapter、host operation、TCP/TLS 意味論を一緒に検証しなければならない。

<a id="original-runtime-estimates-not-a-current-compatibility-or-footprint-matrix"></a>

### 当初の runtime 見積り（現行の互換性・占有量 matrix ではない）

| Host runtime | サイズ | 注記 |
|---|---|---|
| `wasm3` | ~64 KB | interpreter 候補。host 全体の占有量は未確認 |
| `wasm-micro-runtime`（WAMR）| ~85 KB+ | Interp + AOT、Bytecode Alliance、より広い WASI 対応 |
| wasmtime | ~10 MB | desktop 候補。Porta には native process mode もある |
| Almide で書く独自 runtime | — | 延期。「範囲外」を参照 |

WAMR と wasm3 は version を固定した interface、flash/RAM、hardware のテストを待つ候補のままであり、この文書は配備 runtime を選定しない。

---

<a id="original-desktopmcu-comparison-porta-architecture-update-above"></a>

## 当初の desktop/MCU 比較（Porta architecture 更新は上記）

| 軸 | `porta`（既存）| `porta-embedded` |
|---|---|---|
| Target | desktop / server | MCU / IoT device |
| Host runtime | wasmtime（~10 MB）| WAMR / wasm3（~64–85 KB）|
| OS 層 | macOS / Linux + `sandbox-exec` | bare metal、RTOS なしまたは薄い RTOS |
| 用途 | AI agent sandbox、Claude Code 制御 | sensor / actuator / telemetry agent |
| Capability | FS / Network / Exec | GPIO / I²C / SPI / Network |
| Manifest | `porta.toml` + `manifest.json` | 同じ形式、parser を共有 |

manifest と capability model の共有は設計目標。同等の強制と変更なし artifact の移植性には、選択した各 host で肯定・否定テストが必要。schema の共有だけからは導けない。

---

<a id="phases"></a>

## 段階

<a id="phase-0--wasi-preview-2-bindings-23-weeks"></a>

### Phase 0 — WASI Preview 2 bindings（2–3 週間）

- Almide stdlib に `wasi-sockets-tcp`、`wasi-clocks`、`wasi-random` binding を加える。
- desktop wasmtime で Almide の平文 TCP echo client を動かす。
- 目標：Almide ↔ WASI Preview 2 接続が end-to-end に動く証拠。

<a id="phase-1--mqtt-publish-agent-poc-34-weeks"></a>

### Phase 1 — MQTT publish agent PoC（3–4 週間）

- pure Almide で MQTT 3.1.1 QoS 0 publisher を実装（TLS はまだなし）。
- 目標：WASM guest < 8 KB。
- desktop wasmtime から公開 broker へ publish し、message rate と binary size を測る。
- 同等の C + lwIP 実装と比較する。

<a id="phase-2--tls-integration-blocked-on-wasi-tls"></a>

### Phase 2 — TLS 統合（`wasi-tls` 待ち）

- `wasi-tls` 提案が安定したら Almide stdlib binding を加える。
- host 側 TLS を使い、TLS 上に MQTT を重ねる。
- 目標：同じ agent logic で WASM guest < 10 KB。

<a id="phase-3--mcu-host-and-first-hardware-run-12-months"></a>

### Phase 3 — MCU host と初の hardware 実行（1–2 か月）

- WAMR と wasm3 のサイズ、RAM 占有量、WASI Preview 2 operation 対応を benchmark 比較する。
- ESP32-S3、STM32U5、RP2350 のいずれかで選んだ runtime を立ち上げる。
- message rate、消費電力、peak RAM を記録する。

<a id="phase-4--porta-embedded-public-release-23-months"></a>

### Phase 4 — `porta-embedded` 公開 release（2–3 か月）

- `porta` から `porta-embedded` を分離する（別 repo か subdirectory かは未定）。
- 両形式をカバーする統一 `porta.toml` schema。
- agent 例（sensor publisher、actuator subscriber、telemetry relay）。
- 最初の `v0.1.0` tag。

<a id="phase-5--vendor-hal-wrappers-ongoing"></a>

### Phase 5 — vendor HAL wrapper（継続）

- ESP-IDF、STM32Cube、nRF Connect SDK の薄い adapter。
- host 側 WAMR 統合 template。
- desktop `porta` と開発者体験を揃える。

---

<a id="differentiator-axes"></a>

## 差別化の軸

graphics stack の strategy 文書と `ml-inference.md` は、新 package を同じ五軸で評価する。`porta-embedded` は次のとおり：

| 軸 | 立ち位置 |
|---|---|
| Binary size | 目標：guest < 10 KB、host + guest < 110 KB。ここでは未測定。|
| LLM による記述 | 制御された編集 task で C と比較する仮説。|
| Zero-copy | copy と ABI serialization は選んだ host 境界に依存。測定が必要。|
| Cross-host | 明示的にテストした desktop/MCU host で同じ artifact が動くことを目指す。|
| Capability security | MCU host での強制の提案。desktop 制御から暗黙に成立するものではない。|

---

<a id="scope-out"></a>

## 範囲外

- **bare-metal RTOS の作成。** この application/host 分担の外。上のサイズ見積りは数学的な不可能性証明ではない。
- **Almide で software TLS を書き直すこと。** 新実装が必要だと主張するのではなく、crypto を委譲する提案。
- **Almide で書く独自 WASI runtime。** 長期的には興味深いが、`no_std` codegen、inline asm、linker section 制御が必要で、当時はすべてない。代わりに WAMR / wasm3 を借りる。
- **MCU の ML 推論。** `ml-inference.md` にある。後で基盤を共有できる。
- **MCU の graphics。** `obsid/docs/strategy.md` にある。後で基盤を共有できる。
- **Ethernet / Wi-Fi driver。** Almide でなく host の責任。

---

<a id="resume-conditions"></a>

## 再開条件

すべて満たしたら、この項目を `active/` へ移す：

1. 既存 `porta` が v1.0 を release した。
2. 少なくとも一つの host runtime で WASI Preview 2 sockets が安定。
3. Almide stdlib に WASI Preview 2 binding がある（Phase 0 の前提）。
4. 少なくとも一つの graphics stack package が v1.0 に凍結された（compiler が十分安定した兆候）。

---

<a id="related-roadmap"></a>

## 関連 roadmap

- **`on-hold/ml-inference.md`** — 同じ Almide 差別化軸を ML 推論へ適用。両者は WASI guest + 小型 host architecture に依存し、`wasm-simd128` intrinsic とサイズ最適化 codegen 作業を共有できる。
- **`obsid/docs/strategy.md`**（外部 repo）— graphics stack 戦略。§C2「抽象 host interface」は `porta-embedded` が必要とする同じ設計原則。

三つの取り組みすべてが実現すれば、統一した Almide の立ち位置はこうなる：**「desktop、browser、embedded で同じ code を動かす小型 WASI guest。agent、renderer、推論のいずれにも。」**
