# Claim wording: Perceus phrasing and the byte-identity guarantee scope

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](claim-wording-perceus-byte-identity.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.zh-CN.md)

> Non-normative reader copy of a historical roadmap marked done on 2026-07-15. Original measurements, public-copy guarantees and removed-API statements are historical source claims, not independently verified current guarantees. In particular, later documents record changes to the fan time APIs; this record is not reconciled into current API advice. The original remains authoritative for its history; this copy is not automatically synchronized. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/claim-wording-perceus-byte-identity.md)

Two wording adjustments intended to make public claims (tweets / README claims block) fully match measurements. GitHub issue: [#773](https://github.com/almide/almide/issues/773). Both wordings are **settled**, and the README updates are complete.

<a id="1-perceus-rc-で自動管理perceus-方式の所有権推論で自動管理確定"></a>

## 1. “Automatically managed by Perceus RC” → “Automatically managed by Perceus-style ownership inference” [settled]

Strictly:
- **wasm leg**: executes Perceus MIR decisions using RC (rc_inc/rc_dec + free-list).
- **Rust leg**: implements the same Perceus MIR decisions (Alloc/Dup/Drop/Consume, certified by `verify_ownership`) using Rust clone/drop. RC is the **execution mechanism** used on the wasm side.

Settled wording: “Memory management is automatically decided by Perceus-style ownership inference (wasm executes RC; Rust output realizes the same decisions using Rust drop).” This wording applies to both targets.

README update: rewrote Memory Safety around “decided by Perceus-style ownership inference / what differs per target is only the *execution mechanism*”. The trust-spine ladder (#764) reference also changed to “shared scalar and list ops already render on both targets from the same MIR”, reflecting the state after rung 4 shipped.

<a id="2-出力バイト一致を保証の範囲明示確定"></a>

## 2. Make the scope of “guaranteed output-byte identity” explicit [settled]

Settled wording: “Byte identity of observable output (stdout/stderr/exit) is **continuously guaranteed** by the contract ledger (C-001..C-133, zero flagged), cross-target differential gates (246 spec/wasm_cross fixtures), and verification of real org projects on both targets.”

Explicit exclusions, managed in the ledger:
- Intrinsic nondeterminism is certified through contracts for “deterministic invariants”, such as the range invariant of C-112 `random.int`. The sole wall-clock surface (fan.timeout) was removed under C-006.
- APIs not implemented on wasm (fs.glob, etc.) **refuse compilation or execution**, rather than outputting wrong bytes (honest wall).

README update: added three items under “continuous, with an explicit scope” in Equivalence Claim: in scope / nondeterministic sources / not-yet-implemented APIs.

<a id="確定版発信文ツイート等"></a>

## Settled public wording (tweets, etc.)

> I'm building a language for exactly that need. Programmers write neither ownership nor lifetimes. Memory management is automatically decided by Perceus-style ownership inference (wasm executes RC; Rust output realizes the same decisions through drop). It compiles to Rust/WASM while retaining Rust-style type discipline with no exceptions and exhaustive match. Observable output (stdout/stderr/exit) byte identity is continuously guaranteed through the contract ledger and differential gates. Please take a look if you're interested.

Short form, when character counts are tight:

> I'm building a language where you write neither ownership nor lifetimes. Memory management is automatically decided by Perceus-style ownership inference (RC on wasm; the same decisions through drop on Rust). With no exceptions and exhaustive match, observable-output byte identity between Rust/WASM outputs is continuously guaranteed by the contract ledger and CI gates. Please take a look if you're interested.

<a id="背景言い切りを支える事実"></a>

## Background: facts supporting the wording

- The last known divergence (snapshot aliasing of mutable module var — `--no-verified` v0-wasm aliases, while native/default v1 uses COW) was **resolved on the default path** at e088c25d and pinned as C-033 fixture `spec/wasm_cross/module_var_alias_cow.almd`. Divergence remains only under explicit opt-out (`--no-verified`).
- The checker enforces exhaustive match with E010 (confirmed by measurement).
