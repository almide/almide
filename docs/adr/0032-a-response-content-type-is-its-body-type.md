# ADR-0032: A response's Content-Type is its body's type — `Body` gives each body type its media type, header writes cannot name Content-Type, and a script-capable type is built only by its safe module

- **Status**: Accepted (2026-10-29). The owner ruled ○ on the direction and on Q2, × on Q7; the rest were decided as recommended (○), including Q14–Q16 below. See [Owner decisions](#owner-decisions-).
- **Date**: 2026-10-29
- **Scope**: the `http` response surface (`response`, `html`, `json`, `with_headers`, `set_header`, `set_body`, `set_body_bytes`, `serve`), the `html` module's `SafeHtml`, owners of media types outside the stdlib and the application's grant of them (D7), the media type of a body, security-table row 11e, contract C-275.
- **Related**:
  - `docs/roadmap/active/security-by-construction.md` row 11e (HTML injection). Its interim mechanism, a run-time guard on header strings plus E115, is in `stdlib/http.almd` and `crates/almide-frontend/src/check/content_type_guard.rs`. It stays until this surface lands and is then removed.
  - R-L0-37 (a concrete type is a composition of protocols); the builtin-protocol table in `crates/almide-frontend/src/canonicalize/protocols.rs` (`HttpMessage`, `Encode` / `Decode` / `Codec`).
  - C-367 / C-407 (the serve core owns the framing fields and adds `X-Content-Type-Options: nosniff`).

## Context

Row 11e's interim fix refuses an HTML (and XML) Content-Type written as a String header next to a String body. It works, but it is a guard on a *string*, so it has three structural weaknesses:

1. **http holds a media list.** Which types are script-capable is a property of the document format, but the list lives in the transport module (`__http_response_html_types`). A new script-capable format means editing http.
2. **The body's meaning is a header string.** `with_headers(200, s, [("Content-Type", t)])` says the body is a `t` document, whatever `s` is. A type is the one thing the checker can follow, and the body has none: it is always `String`.
3. **A response can carry no Content-Type at all.** `with_headers` without one leaves the browser to sniff. `nosniff` (C-407) narrows sniffing but does not define a type.

The owner's ruling: the Content-Type comes from the body's type, never from a header string; safety lives in the format modules (`html`, and packages such as almide/svg under D7), and http knows no media list.

## Decision

### D1 — `Body`: a body type knows its media type

A builtin protocol `Body` (the noun family `Encode` / `Decode` / `HttpMessage` already uses) has two requirements:

- `T.media_type() -> String`: the Content-Type a response with this body carries;
- `body.to_text() -> String`: the body on the wire. A built response holds its body as text until #4311 gives it an exact bytes slot, so the requirement is the text (a Bytes body decodes as lossy UTF-8, Q16). It was `to_bytes` in the proposal; as text, a String body costs no round trip and `string` does not reach `bytes` (the #4289 reach gate in `bundled_reach.rs`). Both requirements are per-type (`__self_media_type`, `__self_to_text`), so neither is user surface.

| Body type | Content-Type | Declared in |
|---|---|---|
| `String` | `text/plain; charset=utf-8` | builtin conformer |
| `Bytes` | `application/octet-stream` | builtin conformer |
| `SafeHtml` | `text/html; charset=utf-8` | `html` (`type SafeHtml: Body`) |
| a package's type | its declared literal | the package, granted by the application when the type is script-capable (D7) |
| `Value` (JSON) | `application/json` | `json` (sub-decision Q3) |
| `Typed` (new) | the type it was built with | `http` (D4) |

### D2 — a response is built from a `Body`

`http.response[B: Body](status, body: B) -> HttpResponse` sets Content-Type from `B.media_type()` and the body from `body.to_bytes()`. `set_body[B: Body](resp, body)` replaces both, so a String written into an HTML response turns it into `text/plain`. Nothing is refused there, because nothing can be mislabelled. A fetched response keeps the type it arrived with, as the upstream sent it, until `set_body` replaces it (sub-decision Q11).

### D3 — Content-Type is owned by the body, so header writes cannot name it

`Body` declares the header it owns (`@owns_header("Content-Type")` on the protocol declaration). `set_header` / `with_headers` refuse a name the protocol owns:

- at check time when the name is a literal (E116);
- at run time otherwise (an abort naming `http.response`).

The rule reads the protocol's declaration, never a list in http. The framing fields the serve core owns (C-367) are a separate, existing rule.

### D4 — every other media type has a typed way in, and a script-capable one has none

`http.typed(media_type: String, body: Bytes) -> TypedBody` builds a body of any well-formed media type, written as a literal (Q14): the checker reads it, so nothing about media types exists at run time. A type another `Body` type **owns** is refused.

Ownership is declared on the safe types themselves:

- `@media_owns("text/html", "application/xhtml+xml")` on `SafeHtml`'s media fn;
- `@media_owns(...)` on a package type's media fn, under D7.

Which types are script-capable is declared once, by `@script_capable(...)` in `stdlib/html.almd`: `text/html`, `application/xhtml+xml`, `image/svg+xml`, `*/xml` and `*/*+xml` (any XML document can carry the XHTML namespace). `http.typed` refuses a script-capable type whether or not anyone owns it, so XML (Q7 ×, #4310) and, until almide/svg ships, SVG are owned-but-unbuildable.

The attribute sits on the type's `Body` requirement fn (`__safe_html_media_type`), the declaration that states the type's media type.

Patterns follow RFC 6839: `*` is any type, and `*<suffix>` matches a longer subtype ending in `<suffix>`. The refused set is therefore the union of the declarations.  A computed, malformed or owned media type is E117 at check time, and so is naming `http.typed` as a value. `Body` was sealed (Q15); D7 replaces that with grants.

### D5 — the safe modules

- `html` is unchanged: context builders, `raw` as the named opt-out.
- SVG is not in the stdlib. The external package almide/svg owns `image/svg+xml` under D7, with the builder set this ADR first proposed for a stdlib `svg` (`escape`, `attr` and `url_attr` refusing `on*`, `style` and script URLs, `element` refusing `script`, `foreignObject` and the animation elements, `raw` as the named opt-out). No standard library surveyed ships SVG; TyXML and Trusted Types keep SVG beside HTML as one more trusted markup.
- `xml`: Q7 ×, #4310.

### D7 — owners outside the stdlib, granted by the application (Trusted Types with a CSP allowlist)

`Body` is open: a package type may conform and may own media types with `@media_owns`. Ownership of a script-capable type is an authority to put script in front of a browser, so it needs the application's consent:

```toml
[trust]
media = { "almide/svg" = ["image/svg+xml"] }
```

- **Who is named.** The key is the dependency's source `owner/repo` (github.com implied; `host/owner/repo` for another host; a path dependency by its `[dependencies]` name). A dependency name can be pointed at another repository, a source cannot (Q17).
- **What needs a grant.** A dependency whose `@media_owns` overlaps the script-capable set and is not granted that pattern fails the build (E118), naming the package, the type and the line to add. A grant naming a type the package does not declare is E118 too. Owning only other types (`text/csv`) needs no grant. Stdlib `html` is granted implicitly; the application's own types need no grant, as with native code (Q18), and are listed.
- **The media type is a literal.** A non-stdlib `Body` type's media fn is a string literal; a script-capable one must be covered by the fn's own `@media_owns` (Q19, E118). So a granted `text/csv` package cannot serve `text/html` at run time.
- **One owner per type.** Two owners whose patterns can match a common type are E118 (Q22), so `http.typed`'s refusal names exactly one owner.
- **Auditing.** `almide deps` lists every media owner: html, each granted package with its patterns and grant line, and the application's own (Q23).
- **One code.** Every media-ownership error is E118 (Q20); its earlier meaning, "a user type declares Body", was never released.

Threat model: a transitive dependency cannot silently become an HTML authority. Adding, upgrading or swapping a package that claims a script-capable type stops the build until the application names that package and that type; the grant is keyed by source, so a renamed or re-pointed dependency is not covered by an old grant.

### D6 — migration

- A dialect-epoch break (Q12).
- `almide fix` rewrites the old spellings whose Content-Type is a literal:
  - `http.html(st, h)` → `http.response(st, h)`;
  - `with_headers(st, s, [("Content-Type", "text/plain…"), …])` → the String body and the remaining headers;
  - `application/json` → `http.json`;
  - any other literal type → `http.typed(t, bytes.from_string(s))!`.
- A computed Content-Type is not rewritable and gets E116's hint.
- The interim guard and E115 are removed in the same change that lands D2–D4. E115 retires with its docs kept, like E039 / E040.

## Cross-language comparison

| | Content-Type comes from | Can a header string relabel the body? | Script-capable text built by |
|---|---|---|---|
| **axum** (Rust) | the `IntoResponse` type: `Html<T>`, `Json<T>`; `String` → `text/plain; charset=utf-8`, `Bytes` → `application/octet-stream` | yes: `([(CONTENT_TYPE, "text/html")], s)` | nothing; `Html(s)` takes any string |
| **Go** `html/template` + `net/http` | `Header().Set`, else sniffed (`DetectContentType`) | yes | contextual auto-escape; `template.HTML` is the typed opt-out |
| **TyXML** (OCaml) | the server library (Dream `Dream.html`) | yes, in the server | typed HTML/SVG trees, valid by construction |
| **lucid** + servant (Haskell) | the endpoint's content-type list (`Get '[HTML] (Html ())`) | no, for typed endpoints | `Html` monad escapes text; `toHtmlRaw` opt-out |
| **Trusted Types** (browser) | (sink side) `innerHTML` and friends take only `TrustedHTML` | — | policies are the only constructors, enforced by CSP |
| **Rails** | `render html:` / `render plain:` / `send_data type:` | yes: `send_data(s, type: "text/html")` | `SafeBuffer`, auto-escaped ERB, `html_safe` opt-out |
| **Almide (this ADR)** | the body's type (`Body`) | **no**: E116 / abort | `html` builders, or a package owner the application granted (D7); `http.typed` refuses an owned or script-capable type |

What Almide takes:

- axum's type-to-header table;
- servant's rule that the type, not the handler, decides;
- Trusted Types' rule that a script-capable value has constructors only in a policy module;
- TyXML's typed SVG, as the shape of the almide/svg package.

What it closes and none of them do: the header relabel.

### Who may mint a trusted type (D7)

| | Who can mint the trusted type | Who consents | Visible where |
|---|---|---|---|
| **Trusted Types** + CSP `trusted-types` | any script calling `trustedTypes.createPolicy(name, …)` | the page: the `Content-Security-Policy: trusted-types a b` directive lists the policy names allowed; any other name throws | the response header |
| **Rust** `unsafe impl` | any crate, for its own type (`unsafe impl Send for T`) | the crate's author, by writing `unsafe`; the user of the crate does not | the impl, greppable |
| **Go** `template.HTML` | any package, by a conversion | nobody | nowhere |
| **Rails** `html_safe` | any gem, by a method call | nobody | nowhere |
| **Almide (D7)** | a package whose type declares `@media_owns` | the application: `[trust] media` names the package source and the media type, or the build stops (E118) | `almide.toml`, and `almide deps` |

Trusted Types is the model: a policy is a named constructor, and the deployment, not the library, lists the names it allows. Almide moves the list from the response header to the manifest, so it is checked before the program runs, and keys it by package source rather than by a name the package chooses itself.

## Owner decisions (○×)

Answers (2026-10-29): Q1 ○, Q2 ○ (owner), Q3 ○, Q4 ○, Q5 ○ as amended by Q14, Q6 ○, Q7 × (owner; #4310), Q8 ○, Q9 ○, Q10 ○, Q11 ○ (`set_body` keeps its String signature and replaces the type; `http.with_body[B: Body]` takes any body), Q12 ○ (rc1 not tagged; re-checked at landing), Q13 ○, Q14 ○, Q15 ○, Q16 ○ (#4311). A later ruling the same day (owner ○) replaced Q15 and Q8: `Body` is open to packages under grants (D7), and SVG leaves the stdlib for the almide/svg package; Q17–Q23 ○ as recommended.


- **Q1.** The protocol is named `Body`, with requirements `T.media_type()` and `to_bytes`. Recommendation: ○.
- **Q2.** One constructor, `http.response[B: Body]`. `http.html` is removed (its `SafeHtml` goes through `response`), and `with_headers` takes a `Body` instead of a `String`. Recommendation: ○.
- **Q3.** JSON is typed: `json.Value` conforms to `Body` as `application/json`. `http.json(status, String)` stays as the textual form, because JSON served as JSON with nosniff runs no script. Recommendation: ○.
- **Q4.** Writing Content-Type through `set_header` / `with_headers` is refused: E116 when the name is a literal, an abort otherwise. The alternative is dropping it silently, as the framing fields are. Recommendation: ○ (refuse).
- **Q5.** Other media types go through `http.typed(media_type, Bytes) -> Result[Typed, String]`: `err` for a malformed type or an owned one, E117 when the type is a literal. Recommendation: ○.
- **Q6.** Ownership is declared on the safe types with `@media_owns(patterns…)` (RFC 6839 patterns). `typed` refuses the union of the declarations. Recommendation: ○.
- **Q7.** Active XML (`*/xml`, `*/*+xml` other than xhtml and svg) gets a minimal `xml` module, `SafeXml` built from `xml.element(name, attrs, children)` / `xml.text` / `xml.raw`, which owns those patterns. With ×, a declaration-only owner leaves them unbuildable. Recommendation: × for now. No program in the corpus serves XML. The declaration-only owner keeps the patterns refused, and an `xml` module can come later without a break.
- **Q8.** `svg` mirrors `html`'s builder set: `escape`, `attr`, `url_attr`, `concat`, `empty`, `to_string`, `raw`, plus `svg.element(name, attrs, children)`. Recommendation: ○.
- **Q9.** A `String` body is `text/plain; charset=utf-8`. Today `http.response` seeds `text/plain` with no charset, so this is observable. Recommendation: ○.
- **Q10.** A `Bytes` body is `application/octet-stream`. Recommendation: ○.
- **Q11.** `set_body[B: Body]` replaces the body and the Content-Type together, so writing a String into an HTML response makes it `text/plain` instead of aborting. A fetched response keeps its upstream type until `set_body`. Recommendation: ○.
- **Q12.** The break lands in dialect epoch 15 (0.69.0, still unreleased) rather than a new epoch 16. Recommendation: ○ if v0.69.0-rc1 is not yet tagged when it lands; otherwise epoch 16.
- **Q14.** `http.typed` takes its media type only as a literal; any other argument is E117, so no media list exists at run time. Recommendation: ○.
- **Q15.** `Body` is sealed: only stdlib declarations conform; a user `type T: Body` is E118. Recommendation: ○. *Replaced by D7.*
- **Q16.** A Bytes body passes through today's String-bodied response rep, so non-UTF-8 bytes are decoded lossily; documented and filed as #4311. Recommendation: ○.
- **Q13.** A response always carries a Content-Type: one built without a body is `String ""`, `text/plain; charset=utf-8`. Today `with_headers(st, s, [])` sends none. Recommendation: ○.

- **Q17.** A media grant names the dependency's source `owner/repo` (`host/owner/repo` off github.com; a path dependency by its `[dependencies]` name). Recommendation: ○.
- **Q18.** The application's own types need no grant, as with native code, and are listed. Recommendation: ○.
- **Q19.** A non-stdlib `Body` type's media fn is a string literal, covered by its own `@media_owns` when script-capable. Recommendation: ○.
- **Q20.** E118 (unreleased) is reused for every media-ownership error. Recommendation: ○.
- **Q21.** The script-capable set is declared once, in `stdlib/html.almd`; html owns only `text/html` and `application/xhtml+xml`. Recommendation: ○.
- **Q22.** Two owners whose patterns can match a common type are duplicates. Recommendation: ○.
- **Q23.** `almide deps` gains a media-owners section; no new command. Recommendation: ○.

## Implementation plan (after the decisions)

1. als first: amend C-275 to D1–D4 and add a fixture per body type plus the refusals. Advance the pin.
2. The `Body` builtin protocol and its conformers. `@owns_header` / `@media_owns` read off declarations by the checker and the run time. E116, E117.
3. The media grant (D7): `[trust] media`, the dependency survey, E118, and the owners section of `almide deps`. SVG is the almide/svg package's.
4. The new http surface, then `almide fix` rewrites and the epoch break, in the same change as the removal of the interim guard and E115.
5. Gates: a matrix test that every `Body` conformer's media type is the one its declaration states; that `http.typed` refuses exactly the union of `@media_owns`; and that no header writer reaches Content-Type. Negative controls for each.
