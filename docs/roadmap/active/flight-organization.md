# Flight-grade: the organization and track-record rungs

The [flight-grade program](https://github.com/almide/almide/issues/586) has three
sections. Section A — **engineering** — stays in the issue tracker, because each
item is code, proof, or a gate someone can land. Sections B and C are not: they
are a legal entity, an engagement plan, a funding sequence, a first customer.
They were filed as issues #577–#585 and each one said so in its own body
(*"Not an engineering item; tracked here for program completeness"*).

Nine such issues in a queue of engineering work make the queue lie about how much
of it is actionable. They live here instead. **The umbrella issue #586 keeps its
checklist** — this document holds the detail those issues carried, verbatim in
substance, so closing them lost nothing.

Status markers are the ones the issues carried when they were folded in
(2026-08-13): ❌ not started, ◑ partially addressed.

---

## B — Organization

### B1. A legal entity able to sign dossiers and carry liability  ❌
*(was #577)*

Certification authorities and primes contract with entities, not repositories:
audit responses, liability, and dossier signatures require a legal person. The
agent-assisted workflow can produce the evidence; only an entity can sign it.
Ferrocene and AdaCore exist as companies for exactly this reason.

**What**: decide the corporate form and jurisdiction; define what the entity
warrants (toolchain behaviour per dossier) vs. what stays open source.

**Exit criteria**: an entity exists that can sign a release dossier and enter a
support contract.

### B2. A multi-decade LTS policy and versioning commitment  ❌
*(was #578)*

Aircraft programs outlive toolchains: adopters need a written commitment about
how long a pinned toolchain version receives fixes, how Known Problems are
communicated, and how upgrades are qualified.

**What**: an LTS policy document — support horizon per qualified release,
backport rules, deprecation rules consistent with the stability contract, Known
Problems notification channel.

**Exit criteria**: published LTS policy referenced from the dossier.

### B3. Certification-authority relationships (DER / notified-body engagement)  ❌
*(was #579)*

Qualification is negotiated, not submitted: early engagement with designated
engineering representatives / notified bodies shapes what evidence counts
*before* it is produced at scale.

**What**: identify the first target regime (likely the first customer's: a space
agency software standard or a UAS authority before civil avionics), and engage an
experienced consultant/DER for a gap assessment against the dossier.

**Exit criteria**: a written gap assessment from someone who has taken a tool
through qualification.

### B4. Qualification engineering beyond the solo-plus-agents model  ❌
*(was #580)*

Qualification work has irreducibly human parts: audit interviews, signed reviews,
accountable judgment calls. The current model (one human + an agent fleet)
produces evidence at unusual speed but cannot satisfy independence-of-verification
expectations alone.

**What**: define the minimal team — at least one qualification engineer with
prior tool-qualification experience and an independent-verification role — and
document the independence story: who verifies what, and what the agents' output
counts as.

**Exit criteria**: independence-of-verification structure documented and staffed
for the first qualification attempt.

### B5. Funding the climb from trust-market revenue  ◑
*(was #581 — a strategy exists in the trust-layer roadmap)*

Qualification is a multi-year cost centre; the agent/wasm sandbox trust market is
the revenue and evidence engine that pays for it. Same evidence machinery,
earlier buyers — not a detour from the regulated goal but its funding path.

**What**: sequence trust-layer monetization (receipts, capability containment,
the MCP wedge) so dossier-grade evidence accumulates as a product side effect;
earmark the qualification budget explicitly.

**Exit criteria**: a funded plan in which the qualification milestones (B1–B4 and
the #574 data package) have a revenue source.

**Ref**: [trust-layer.md](./trust-layer.md)

---

## C — Track record

### C1. The first regulated-adjacent deployment, to start the service-history clock  ❌
*(was #582)*

Product service history is itself certification currency, and it only accumulates
in real deployments. Smallsat flight software and UAS (specific category) have
materially lower entry barriers than civil avionics and produce exactly the right
kind of operational record.

**What**: identify and win one design partner flying Almide-generated code
(Critical profile once available) in a space or UAS context; instrument for
problem reporting from day one.

**Exit criteria**: Almide code operating in a fielded space/UAS system with
service history being recorded.

### C2. The deployment ladder  ❌
*(was #583)*

Nobody starts at flight controls. The ladder: ground/verification tooling (that
market is also regulated, and nearer) → space/UAS items → low-assurance airborne
items (DAL-D/C class) → higher levels. Each rung produces customers, evidence,
and history for the next.

**What**: write the ladder as a living document — target regimes per rung, what
evidence each rung consumes and produces, and the promotion criteria between
rungs.

**Exit criteria**: ladder document in `docs/roadmap/` with the current rung
explicit.

### C3. Service-history and problem-reporting record-keeping  ❌
*(was #584 — depends on C1)*

Service history only counts if it is recorded in an audit-ready way: versions in
service, exposure time, problems found, dispositions, and the feedback loop into
Known Problems.

**What**: a lightweight in-service record format tied to the release dossier and
the flagged-contracts ledger; every deployment from C1 onward reports through it.

**Exit criteria**: record-keeping format defined and in use by the first
deployment.

### C4. A published position on certifying machine-written code  ❌
*(was #585)*

Assurance frameworks assume human authors; regulators (the EASA AI roadmap, FAA
counterparts) are actively working out what AI-assisted development means. A
toolchain born machine-writer-first with mechanical evidence is the answer that
side of the table does not yet have — and being part of that conversation early
is how a newcomer gets chosen rather than tolerated.

**What**: a position paper — how spec-keyed contracts, per-build certificates,
and the evidence ladder discharge development-assurance expectations when the
author is a model — and engagement with the relevant working groups.

**Exit criteria**: paper published; at least one regulator / industry
working-group interaction on record.

---

## Critical path (unchanged)

#563 (longest lead) → #572 + #573 (the technical bet: the qualified-generator
model) → **C1** (the history clock) → #574 + **B1** (a signable package).
Everything else feeds these.

**Ref**: [certification-grade.md](./certification-grade.md),
[flight-qualification.md](./flight-qualification.md),
[trust-layer.md](./trust-layer.md)
