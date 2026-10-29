# ADR-0032: A response's Content-Type is its body's type — `Body` gives each body type its media type, header writes cannot name Content-Type, and a script-capable type is built only by its safe module

- **Status**: Accepted (2026-10-29). The owner ruled ○ on the direction and on Q2, × on Q7; the rest were decided as recommended (○), including Q14–Q16 below. See [Owner decisions](#owner-decisions-).
- **Date**: 2026-10-29
- **Scope**: the `http` response surface (`response`, `html`, `json`, `with_headers`, `set_header`, `set_body`, `set_body_bytes`, `serve`), the `html` module's `SafeHtml`, a new `svg` module, the media type of a body, security-table row 11e, contract C-275.
- **Related**:
  - `docs/roadmap/active/security-by-construction.md` row 11e (HTML injection). Its interim mechanism, a run-time guard on header strings plus E115, is in `stdlib/http.almd` and `crates/almide-frontend/src/check/content_type_guard.rs`. It stays until this surface lands and is then removed.
  - R-L0-37 (a concrete type is a composition of protocols); the builtin-protocol table in `crates/almide-frontend/src/canonicalize/protocols.rs` (`HttpMessage`, `Encode` / `Decode` / `Codec`).
  - C-367 / C-407 (the serve core owns the framing fields and adds `X-Content-Type-Options: nosniff`).

## Context

Row 11e's interim fix refuses an HTML (and XML) Content-Type written as a String header next to a String body. It works, but it is a guard on a *string*, so it has three structural weaknesses:

1. **http holds a media list.** Which types are script-capable is a property of the document format, but the list lives in the transport module (`__http_response_html_types`). A new script-capable format means editing http.
2. **The body's meaning is a header string.** `with_headers(200, s, [("Content-Type", t)])` says the body is a `t` document, whatever `s` is. A type is the one thing the checker can follow, and the body has none: it is always `String`.
3. **A response can carry no Content-Type at all.** `with_headers` without one leaves the browser to sniff. `nosniff` (C-407) narrows sniffing but does not define a type.

The owner's ruling: the Content-Type comes from the body's type, never from a header string; safety lives in the format modules (`html`, a new `svg`), and http knows no media list.

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
| `SafeSvg` (new) | `image/svg+xml` | `svg` (`type SafeSvg: Body`) |
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

- `@media_owns("text/html", "*/xml", "*/*+xml")` on `SafeHtml`'s media fn: any XML document can carry the XHTML namespace, so html owns the markup family, and XML stays unbuildable until an `xml` module (Q7 ×, #4310);
- `@media_owns("image/svg+xml")` on `SafeSvg`'s media fn.

The attribute sits on the type's `Body` requirement fn (`__safe_html_media_type`), the declaration that states the type's media type.

Patterns follow RFC 6839: `*` is any type, and `*<suffix>` matches a longer subtype ending in `<suffix>`. The refused set is therefore the union of the declarations. Adding `SafeSvg` removes SVG from `typed` without touching http. A computed, malformed or owned media type is E117 at check time, and so is naming `http.typed` as a value. `Body` is sealed (Q15): a user type declaring it is E118.

### D5 — the safe modules

- `html` is unchanged: context builders, `raw` as the named opt-out.
- `svg` mirrors it:
  - `svg.escape`, `svg.attr` (refuses `on*`, `style` and URL-valued names, as `html.attr` does);
  - `svg.url_attr` (`href` / `xlink:href`, same scheme rule as `html.url_attr`);
  - `svg.concat`, `svg.empty`, `svg.to_string`, and `svg.raw` as the named opt-out.
- `xml` only if Q7 is ○.

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
| **Almide (this ADR)** | the body's type (`Body`) | **no**: E116 / abort | `html` / `svg` builders only; `http.typed` refuses an owned type |

What Almide takes:

- axum's type-to-header table;
- servant's rule that the type, not the handler, decides;
- Trusted Types' rule that a script-capable value has constructors only in a policy module;
- TyXML's typed SVG, as the shape of `svg`.

What it closes and none of them do: the header relabel.

## Owner decisions (○×)

Answers (2026-10-29): Q1 ○, Q2 ○ (owner), Q3 ○, Q4 ○, Q5 ○ as amended by Q14, Q6 ○, Q7 × (owner; #4310), Q8 ○, Q9 ○, Q10 ○, Q11 ○ (`set_body` keeps its String signature and replaces the type; `http.with_body[B: Body]` takes any body), Q12 ○ (rc1 not tagged; re-checked at landing), Q13 ○, Q14 ○, Q15 ○, Q16 ○ (#4311).


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
- **Q15.** `Body` is sealed: only stdlib declarations conform; a user `type T: Body` is E118. Recommendation: ○.
- **Q16.** A Bytes body passes through today's String-bodied response rep, so non-UTF-8 bytes are decoded lossily; documented and filed as #4311. Recommendation: ○.
- **Q13.** A response always carries a Content-Type: one built without a body is `String ""`, `text/plain; charset=utf-8`. Today `with_headers(st, s, [])` sends none. Recommendation: ○.

## Implementation plan (after the decisions)

1. als first: amend C-275 to D1–D4 and add a fixture per body type plus the refusals. Advance the pin.
2. The `Body` builtin protocol and its conformers. `@owns_header` / `@media_owns` read off declarations by the checker and the run time. E116, E117.
3. `svg` (and `xml` under Q7) with its tests and the semantics manifest.
4. The new http surface, then `almide fix` rewrites and the epoch break, in the same change as the removal of the interim guard and E115.
5. Gates: a matrix test that every `Body` conformer's media type is the one its declaration states; that `http.typed` refuses exactly the union of `@media_owns`; and that no header writer reaches Content-Type. Negative controls for each.
