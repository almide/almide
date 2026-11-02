<a id="secure-by-design"></a>

# Secure by Design

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](secure-by-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](secure-by-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](secure-by-design.zh-CN.md)

> Non-normative reader translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/secure-by-design.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Safety, security, comparative, implementation and measurement claims are retained as statements of that design-time source, without independent execution, timing measurement, audit or certification. They are not new guarantees about current releases. Code blocks and comments remain verbatim; metadata is an inactive reader note.

> Source description (inactive metadata): Proposed layered security model, implemented foundations, and remaining trust boundaries

> **Status: On Hold — design proposal, not a security certification.**
> Reviewed against the repository on 2026-10-31. Opaque types, SafeHtml/SafePath
> helpers and effect/capability analysis provide foundations. They do not establish
> universal web safety or the per-package policy interface proposed below.

<a id="goal-and-limits"></a>

## Goal and limits

Make dangerous output and external access explicit and auditable. Typed wrappers
help only when constructors enforce the intended policy and every relevant sink
requires the wrapper. A wrapper alone does not prevent injection, path traversal,
data exfiltration, or malicious dependencies.

This document preserves the five-layer design and its remaining integration work.
For current guarantees and package behavior, see [security by construction](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md)
and the [package specification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md).

<a id="layer-1-effect-isolation--checked-calls-not-a-sandbox"></a>

## Layer 1: Effect isolation — checked calls, not a sandbox

The checker separates ordinary pure and effect calls. This helps identify effectful
operations, but built-in output exceptions exist. A pure API does not prove absence
of observable behavior, termination, bounded resource use, or harmlessness of a
package. Effect checking is not an isolation boundary for untrusted native code.

**Existing foundation:** effect checking and capability analysis. Their current
scope must not be expanded into a blanket claim that pure functions cannot do I/O.

<a id="layer-2-explicit-external-bridges"></a>

## Layer 2: Explicit external bridges

`@extern` makes some external declarations visible. Standard-library intrinsics,
host imports and native dependencies also cross trust boundaries. Searching for
`@extern` is therefore not a complete security audit, nor is it equivalent to
Rust's `unsafe` boundary.

**Remaining proposal:** platform tags and their policy semantics. The earlier
`@extern(platform: web, ...)` examples were proposed syntax, not evidence that
platform-restricted execution is enforced today. Capability inference already
exists; it does not depend on shipping this proposed tag syntax first.

<a id="layer-3-opaque-types-and-safe-sinks"></a>

## Layer 3: Opaque types and safe sinks

The language supports nominal module-private wrappers using `mod type T = U`.
See [`html.SafeHtml`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/html.almd) and
[`path.SafePath`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/path.almd) for existing helpers.

- `html.escape` escapes text, while `html.raw` deliberately wraps **trusted,
  unescaped** input. SafeHtml therefore does not prove that every value was escaped.
- SafePath construction rejects some unsafe strings. Filesystem containment also
  depends on the sink, symlinks, races and platform semantics.
- A proposed SafeSql wrapper and parameterized database sink require a database
  implementation and a defined validation/encoding contract.
- Framework sinks that require validated wrappers remain separate integration
  work; not all standard-library I/O accepts opaque wrappers exclusively.

**Remaining proposal:** define safe sink signatures, validate constructors and
context-specific escaping, then test that unsafe construction and sink bypasses
are rejected. The earlier `Response.html`/HTML-builder sketches were design
examples, not shipped framework APIs.

<a id="layer-4-package-capability-policy"></a>

## Layer 4: Package capability policy

Capability inference helps describe effects reached by a function. The additional
proposal is a consumer-granted, transitive **per-dependency policy**, for example:

```toml
# Proposed interface, not a current manifest contract.
[dependencies.example]
version = "1.0"
capabilities = ["fs"]
```

> Code-comment reading note: the TOML example explicitly proposes an interface; it is not a current manifest contract.

Before implementing this interface, specify:

- how inferred effects and native/host implementations map to capabilities;
- whether enforcement is static, runtime, or both, including unknown calls;
- transitive dependency grants and diagnostics;
- policy for changes in requested capabilities during updates;
- behavior within granted capabilities (a filesystem grant is not proof of benign use).

Existing capability inference must not be described as unimplemented, or confused
with proof that this proposed package policy is enforced.

<a id="layer-5-supply-chain-identity-and-provenance"></a>

## Layer 5: Supply-chain identity and provenance

The proposal combines locked identity, trusted provenance and capability-change
review. See the current package specification for implemented lock/trust behavior;
the hosted registry remains a [separate proposal](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md).

A content hash detects a mismatch against a trusted expected identity. It does not
prove benign behavior, prevent malicious initial publication or independently
solve typosquatting. Hash verification does not sandbox native build steps. A
registry is one possible distribution mechanism, not a prerequisite for hashing.

**Remaining proposal:** define trusted provenance, identity pinning, update review
and the interaction with native dependency trust. Capability changes and hash
mismatches need explicit policy and diagnostics rather than assumed enforcement.

<a id="implementation-sequence-and-why-this-remains-on-hold"></a>

## Implementation sequence and why this remains on hold

1. **Existing foundation:** opaque types, HTML/path helpers and effect/capability
   analysis. Preserve them without claiming universal safe sinks.
2. **Sink integration:** specify and test safe HTML, SQL, command and filesystem
   boundaries, including trusted escape hatches and validation failure behavior.
3. **Package policy:** define grants, transitive enforcement and external-code
   trust; platform-tag design may contribute but is not the existing analyzer.
4. **Distribution/provenance:** integrate trusted identity and capability-change
   review with package tooling and any future registry.

The remaining work includes policy and research questions, not merely scheduling:
which properties are enforced, which components are trusted, how granted authority
can be abused, and what evidence establishes each guarantee. The language's lack
of JavaScript prototypes or dynamic eval does not constrain an external host or
native dependency by itself.

<a id="intended-protections-and-residual-risks"></a>

## Intended protections and residual risks

No coverage percentage is claimed. The former 100%/95% estimates were aspirations,
not measurements or proofs, and should not be used as security guarantees.

| Boundary | Potential protection | Remaining obligation |
|---|---|---|
| HTML/SQL/command output | Reject incompatible values at typed sinks | Correct escaping/parameterization, constructors and trusted raw inputs |
| Filesystem paths | Reject some invalid path strings | Containment, symlinks, races, platform behavior and authority |
| Dependency effects | Identify and restrict selected capabilities | Complete enforcement and abuse within granted capabilities |
| Package identity | Detect changes from a trusted expected identity | Initial trust, provenance, malicious updates and native build behavior |

Authorization logic, validation correctness, deployment configuration, side
channels, SSRF policy and resource exhaustion still require explicit design and
review. These layers support defense in depth; they do not establish elimination
of OWASP categories or make application security review unnecessary.

## Source caveats

The on-hold status, repository-review date, implemented foundations and remaining proposals are preserved as the source reports them. Proposed policy syntax is not a current manifest contract. The rejected coverage percentages are not restored as guarantees; no claim of certification, universal safety or elimination of application review is added.
