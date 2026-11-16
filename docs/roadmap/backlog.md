# Backlog

Long-term items with no near-term plan, moved here from the issue tracker on 2026-11-17.
Each line names its (closed) issue, which keeps the full discussion. Reopen the issue when work starts.
Items that fit an existing roadmap file sit in that file's "Backlog (moved from issues)" section instead.

## Compiler and toolchain

- #1005 Direct native emit from the MIR trust spine (cranelift) for debug builds: removes rustc from the edit loop; waits on #1003
- #1479 Source-declarable ownership guarantee (an "allocates nothing" marker in the fip/fbip shape), checked against the MIR certificate predicate, with its own error code and cross-target fixture
- #1490 User-code fuzzing (`fuzz` blocks + `almide test --fuzz`, seed-replayable) and user-code coverage, the remaining dev-UX gaps

## Language surface

- #3800 Widen identifiers to a Swift-like set including emoji (beyond UAX #31 / C-387), measured on MSR

## Stdlib

- #2596 Pure-Almide crypto module: Ed25519 verify and ChaCha20-Poly1305 with RFC test vectors byte-identical on native and stock WASI (HMAC already landed in 805dca72ed)
- #4310 Typed xml module (opaque SafeXml body owning the XML media types), wanted once a program needs to serve XML

## Packages and ecosystem

- #2332 Package publication path: documented publish procedure run by CI on a throwaway package, checked-in per-package interface artifact diffed at release, publishability gate
- #2597 `almide doc`: package API reference pages generated from the `.almdi` interface artifact, published per git tag (after #2151 and #2332)

## Measurement and process

- #2146 Public MSR harness: five or more languages under one manifest, third-party reproducible, effect A/B as a second condition (almide-dojo side; the README 90-day gate already landed in #2530)
- #3034 Production loop: agents claim agent-ready issues and open PRs unattended, with CI as the only verifier (after #3032 and #3033)
- #3787 Sharpen the inspection complexity judge: a 3% rms fit cannot separate n from n^1.05 or n log n from n^1.1 (log-log slope test or a wider size range)
