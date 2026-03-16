# Lessons from Ruby's Journey to Production Readiness

Research notes for Almide's PRODUCTION_READY.md. Focused on concrete, actionable takeaways.

---

## 1. "Production Ready" Was Defined by a Killer App, Not a Checklist

Ruby existed for 10 years (1995-2005) as a niche scripting language. Ruby 1.0 (1996) through 1.8 (2003) were technically "usable" but had no meaningful adoption outside Japan. What changed everything was Rails (2005). The language's production readiness was retroactively defined by whether it could run Rails reliably.

Matz himself acknowledged this: the maturity of a Ruby implementation is measured by "the Rails singularity" -- the point where it can run Rails. JRuby, TruffleRuby, and Rubinius were all judged by this metric.

**Almide takeaway:** Do not define production readiness as a feature checklist. Define it as: "Can Almide build X?" where X is a compelling, real application. The CLI-First roadmap is the right instinct -- pick 3-5 concrete tools that real developers would want, and make them work flawlessly. The csv2json, grep-lite, and project-init exercises are a start, but the "Rails singularity" equivalent for Almide might be: **an LLM agent that writes, tests, and deploys its own Almide code**. That is the killer app that proves the "language LLMs write most accurately" thesis.

**Concrete recommendation for PRODUCTION_READY.md:** Replace any "N% of stdlib implemented" metric with "these 5 specific programs compile and run correctly on both Rust and TS targets."

---

## 2. The Stdlib Bloat Trap: Ruby Spent 15 Years Trying to Undo It

Ruby shipped a massive standard library (net/http, webrick, xmlrpc, csv, json, yaml, openssl, etc.) bundled directly into the interpreter. This created two problems:

1. **Release coupling:** Security fixes for WEBrick or OpenSSL required a full Ruby release. Libraries that needed fast iteration were stuck on Ruby's slow annual release cycle.
2. **Maintenance burden:** Core team members had to maintain libraries they didn't use or understand.

Ruby's solution was "gemification" -- a 15-year effort (started ~2011, still ongoing in 2025) to extract stdlib into independently-versioned gems. The taxonomy:
- **Default gems**: Ship with Ruby, can't be uninstalled, but can be upgraded independently via RubyGems
- **Bundled gems**: Ship with Ruby but can be uninstalled; maintained outside core
- **Non-gem stdlib**: Tightly coupled to the VM (e.g., `Fiber`, `Ractor`)

This migration caused real pain. When `net-smtp`, `matrix`, and `csv` became bundled gems in Ruby 3.x, existing code broke with `LoadError` unless users added them to their Gemfile.

**Almide takeaway:** Almide's two-tier stdlib is already the right design:
- **Hardcoded modules** (string, list, map, int, float, math, fs, process, env, io, json, http, random, regex) = Ruby's "non-gem stdlib" -- tightly coupled to codegen
- **Bundled modules** (args, path, time, encoding, hash, term) = Ruby's "default gems" -- written in Almide, independently evolvable

The lesson is: **never move a hardcoded module to bundled unless you have a versioning/upgrade story.** And conversely: **every new module should start as bundled (pure Almide) unless it absolutely requires codegen integration.** Ruby wished it had started this way.

**Concrete recommendation for PRODUCTION_READY.md:** Add a rule: "New stdlib modules MUST be implemented as bundled `.almd` files. A module may only become hardcoded if it requires target-specific codegen that cannot be expressed via `@extern`."

---

## 3. The Encoding Catastrophe: Never Change a Core Type's Semantics

The Ruby 1.8 to 1.9 migration (2007-2009) was the most painful upgrade in Ruby's history. The root cause: **strings changed from byte sequences to encoding-aware character sequences.** This broke virtually every Ruby program that dealt with I/O, databases, or binary data. The community was split for years between 1.8 and 1.9.

Specific pain points:
- `String#each` was removed (strings were no longer `Enumerable`)
- `?c` changed from returning an integer to returning a one-character string
- `String#[]` changed from returning a byte value to returning a substring
- Binary data required explicit `ASCII-8BIT` encoding markers

The migration took the community roughly 3-4 years to complete, and Rails couldn't fully support Ruby 1.9 until Rails 3.0 (2010).

**Almide takeaway:** Almide's `String` is already encoding-opaque (UTF-8 internally, no byte/char confusion exposed). This is correct. But the broader lesson is: **never change the semantics of a core type in a minor version.** If `List`, `Map`, `String`, or `Result` behavior changes, that is a new major version, and migration tooling must exist before the release.

**Concrete recommendation for PRODUCTION_READY.md:** Before 1.0, audit every core type's API surface and freeze it. Post-1.0, core type changes require a new major version with an `almide migrate` tool.

---

## 4. Concurrency: Ship Simple, Evolve Carefully

Ruby's concurrency history is a cautionary tale of incremental complexity:

| Era | Mechanism | Problem |
|---|---|---|
| 1.8 | Green threads (no OS threads) | No parallelism at all |
| 1.9 | OS threads + GVL | Threads exist but can't run Ruby code in parallel |
| 2.0 | Lazy enumerators, refinements | Still no parallelism |
| 3.0 | Fibers (cooperative), Ractors (isolated parallelism) | Ractors are experimental, most libraries incompatible |
| 3.4+ | Ractor improvements | Still "experimental" after 4+ years |

The GVL (Global VM Lock) was introduced as a simplification -- it made C extensions safe and memory management simple. But it became a permanent constraint that Ruby has never fully escaped. Ractors (Ruby 3.0, 2020) were supposed to solve this, but adoption has been nearly zero after 5+ years because:
- Most gems don't work inside Ractors
- No shared mutable data structures (no connection pools, no shared caches)
- GC performance degrades with multiple Ractors
- `require` doesn't work in non-main Ractors

**Almide takeaway:** Almide's `fan` / structured concurrency design (async let, scoped tasks, auto-cancellation) is architecturally sounder than Ractors because it doesn't require ecosystem-wide compatibility changes. But the lesson from Ruby is:

1. **Ship the simple thing first.** Ruby's green threads were "wrong" but let people write concurrent code for a decade. Almide should ship sequential-only (which it has) and add `async let` only when the semantics are proven on the TS target.
2. **Don't ship "experimental" concurrency.** Ractors being "experimental" for 5+ years has destroyed trust. If Almide ships `fan`, it should be non-experimental from day one.
3. **The ecosystem must be concurrent-compatible.** Ruby's problem is that gems weren't designed for Ractors. Almide's `effect fn` annotation already partitions the world -- pure functions are automatically safe for concurrent use. This is a genuine structural advantage.

**Concrete recommendation for PRODUCTION_READY.md:** Concurrency is NOT a prerequisite for production readiness. Sequential CLI tools are the first target. When concurrency ships, it must be non-experimental and all stdlib modules must be compatible from day one.

---

## 5. The Ecosystem Bootstrap: Package Manager Before Registry

Ruby's ecosystem milestones in order:
1. **Language** (1995) -- Ruby itself
2. **RubyGems** (2004) -- package manager and registry
3. **Rails** (2005) -- killer application
4. **RSpec** (2005) -- testing framework
5. **Bundler** (2010) -- dependency resolution and lockfiles
6. **Bundler merged into Ruby** (2019) -- `gem` and `bundle` ship with Ruby

The critical insight: **RubyGems preceded Rails by one year.** Without a package distribution mechanism, Rails could not have been distributed. But RubyGems itself was simple -- it didn't need Bundler's lockfile resolution to be useful. `gem install rails` was enough.

**Almide takeaway:** Almide's package story is currently "on hold" (from the CLI-First roadmap). This is probably fine for the CLI-First phase, but the lesson from Ruby is that a minimal package mechanism must exist before the killer app can emerge. The minimal viable version is not a registry -- it's:
1. `almide.toml` with `[dependencies]` (already designed)
2. `almide install github.com/user/lib` (git-based, no registry)
3. A lockfile

This is closer to Go modules (2019) than RubyGems, and it's simpler to implement.

**Concrete recommendation for PRODUCTION_READY.md:** Add "minimal package resolution (git-based, no registry)" as a prerequisite for production readiness. A registry can come later.

---

## 6. "Developer Happiness" vs. "LLM Accuracy" -- Different Optimization Targets

Matz explicitly optimized Ruby for "developer happiness" -- making the programmer smile. This led to:
- Multiple ways to do things (Perl heritage: TIMTOWTDI)
- Magic globals (`$_`, `$1`, `$&`)
- Open classes (monkey-patching)
- Heavy metaprogramming (`method_missing`, `define_method`)
- DSLs that look like English (`5.times do ... end`, `describe ... it ...`)

Matz's biggest regret was taking too many features from Perl: "I should have thought more about the features I took, since Ruby itself is not really a script language anymore." He also regrets mutable-by-default objects, the primitive thread implementation, and `alias`/`undef`.

**Almide takeaway:** Almide's "minimal thinking tokens" philosophy is the exact inverse of Ruby's "developer happiness." This is correct for the LLM target audience, but the lesson from Ruby is that **the optimization target must be stated clearly and repeatedly**, because contributors and users will constantly push toward "developer happiness" features (syntactic sugar, multiple ways to do things, convenience aliases).

Ruby's specific mistakes that Almide has already avoided:
- Multiple ways to do things (Almide: one way per construct)
- Mutable by default (Almide: immutable values)
- Magic globals (Almide: no globals)
- Open classes / monkey-patching (Almide: no metaprogramming)
- Operator overloading (Almide: fixed operator semantics)

**Concrete recommendation for PRODUCTION_READY.md:** Include a "Rejected Patterns" section that lists features explicitly excluded and why, so future contributors don't re-propose them. Ruby's experience shows that without explicit rejection, popular features creep in.

---

## 7. Versioning: Annual Releases Build Trust, Cultural Bumps Build Hype

Ruby adopted annual Christmas releases starting with Ruby 2.1 (2013). Each version is maintained for 3 years + 3 months. Major version bumps (2.0, 3.0, 4.0) are cultural milestones, not purely semver-breaking changes:
- **Ruby 2.0** (2013): 20th anniversary, only 5 incompatibilities with 1.9.3
- **Ruby 3.0** (2020): "Ruby 3x3" -- 3x performance improvement goal
- **Ruby 4.0** (2025): 30th anniversary, ZJIT compiler, Ruby::Box isolation

The community initially had no formal versioning policy. Starting with 2.1.0, Ruby adopted a policy "similar to semantic versioning" but not identical -- minor versions may include API incompatibilities. This caused some confusion but worked well enough because the annual cadence made upgrades predictable.

**Almide takeaway:** Don't obsess over semver purity before 1.0. But do establish:
1. A predictable release cadence (even if quarterly, not annual)
2. A clear "this is 1.0" threshold (= the programs from takeaway #1 all work)
3. Post-1.0: minor versions must not break existing programs

**Concrete recommendation for PRODUCTION_READY.md:** Define 1.0 as: "All spec/ tests pass on both targets, the 5 showcase programs compile and run correctly, the stdlib API surface is frozen, and `almide test` is the verification command."

---

## 8. Performance Is a Version Goal, Not a Launch Requirement

Ruby was notoriously slow for its first 15 years. Ruby 1.8 was pure interpretation. Ruby 1.9 (2007) introduced YARV (bytecode VM) -- a major speedup. Ruby 3.0 (2020) set "3x3" as a goal (3x faster than 2.0). Ruby 3.x delivered this through:
- YJIT (JIT compiler from Shopify, merged in 3.1)
- ZJIT (new JIT in 4.0)
- GC improvements

Despite being slow, Ruby powered GitHub, Shopify, Basecamp, and hundreds of production apps. Performance was never the reason people chose Ruby, and it was rarely the reason they left (those who needed performance used JRuby on the JVM or wrote C extensions).

**Almide takeaway:** Almide compiles to Rust, so performance is already handled by the target language. But the lesson applies to compilation speed: `almide run` (via TS/Deno) should be fast enough for development iteration. `almide build` (via Rust) can be slow because it's for deployment. This two-path strategy mirrors how Ruby developers used MRI for development and JRuby for production.

**Concrete recommendation for PRODUCTION_READY.md:** Do not list "compilation performance" as a production-readiness requirement. Instead, require: "`almide run app.almd` completes in under 2 seconds for a 500-line program."

---

## Summary: 8 Takeaways Ranked by Priority for PRODUCTION_READY.md

| Priority | Takeaway | Action |
|---|---|---|
| P0 | Define production readiness as "these N programs work," not a feature checklist | Write 5 showcase programs as the acceptance test |
| P0 | Freeze core type APIs before 1.0 | Audit String, List, Map, Result, Option surfaces |
| P1 | Concurrency is not a launch blocker | Ship 1.0 without `fan`/`async let` if needed |
| P1 | New stdlib = bundled `.almd` by default | Codify the rule; only hardcode when codegen-required |
| P1 | Minimal package resolution before killer app | `almide install` from git, lockfile |
| P2 | Predictable release cadence post-1.0 | Quarterly releases, 1 year support window |
| P2 | Maintain a "Rejected Patterns" list | Prevent feature creep from Ruby-style convenience |
| P3 | Dev-loop speed over build speed | `almide run` < 2s is the metric, not `almide build` |

---

## Sources

- [Ruby Evolution - Ruby Changes](https://rubyreferences.github.io/rubychanges/evolution.html)
- [Ruby Performance Evolution](https://dev.to/daviducolo/ruby-performance-evolution-from-10-to-today-4hc0)
- [Ruby (programming language) - Wikipedia](https://en.wikipedia.org/wiki/Ruby_(programming_language))
- [Ruby Standard Gems](https://stdgems.org/)
- [Default Gems and Bundled Gems - RubyGems Guides](https://guides.rubygems.org/default-gems-and-bundled-gems/)
- [Long journey of Ruby standard library - RubyKaigi 2024](https://rubykaigi.org/2024/presentations/hsbt.html)
- [Gemification plan of Standard Library on Ruby](https://www.slideshare.net/hsbt/gemification-plan-of-standard-library-on-ruby)
- [The Practical Effects of the GVL on Scaling in Ruby](https://www.speedshop.co/2020/05/11/the-ruby-gvl-and-scaling.html)
- [What's The Deal With Ractors?](https://byroot.github.io/ruby/performance/2025/02/27/whats-the-deal-with-ractors.html)
- [Ruby Ractor limitations and real-world usage](https://rubytalk.org/t/ruby-talk-444105-ractor-status-are-they-used/76145)
- [A History of Bundles: 2010 to 2017](https://andre.arko.net/2017/11/16/a-history-of-bundles/)
- [RubyGems - Wikipedia](https://en.wikipedia.org/wiki/RubyGems)
- [The Ruby Ecosystem in 2025](https://thomaspowell.com/2025/09/30/the-ruby-ecosystem-in-2025/)
- [The Philosophy of Ruby - Artima](https://www.artima.com/intv/rubyP.html)
- [Why Matsumoto Created Ruby](https://medium.com/@dev.ajayagrawal/why-matsumoto-created-ruby-the-language-born-for-developer-happiness-28b0a115c20f)
- [Ruby pioneers come clean on the language's shortcomings](https://www.infoworld.com/article/2240439/ruby-pioneers-come-clean-on-languages-shortcomings.html)
- [Ruby backward compatibility debate](https://gist.github.com/e2/ac32569852cbd31f7da637500174d907)
- [Ruby version policy changes with 2.1.0](https://www.ruby-lang.org/en/news/2013/12/21/ruby-version-policy-changes-with-2-1-0/)
- [Ruby 4.0.0 preview3 Released](https://www.ruby-lang.org/en/news/2025/12/18/ruby-4-0-0-preview3-released/)
