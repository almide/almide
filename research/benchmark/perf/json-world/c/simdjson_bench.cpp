// simdjson has no mutable DOM, so this bench is labelled:
//   build     = a std::vector of plain structs (not a JSON DOM),
//   stringify = simdjson::builder::string_builder (the library's writer),
//   parse     = dom::parser into a DOM (the text copied into a padded_string,
//               counted in parse), then sum walks that DOM.
// JW_ONDEMAND=1 at compile time swaps parse+sum for the On-Demand API
// (no DOM: the sum is taken while iterating; parse reports the fused time).
// Allocation counts: every operator new / new[] (simdjson allocates through them).
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <new>
#include <string>
#include <string_view>
#include <vector>
#include "phases.h"
#include "simdjson.h"

void *operator new(std::size_t n) { jw_allocs++; void *p = std::malloc(n ? n : 1); if (!p) std::abort(); return p; }
void *operator new[](std::size_t n) { jw_allocs++; void *p = std::malloc(n ? n : 1); if (!p) std::abort(); return p; }
void *operator new(std::size_t n, const std::nothrow_t &) noexcept { jw_allocs++; return std::malloc(n ? n : 1); }
void *operator new[](std::size_t n, const std::nothrow_t &) noexcept { jw_allocs++; return std::malloc(n ? n : 1); }
void operator delete(void *p) noexcept { std::free(p); }
void operator delete[](void *p) noexcept { std::free(p); }
void operator delete(void *p, std::size_t) noexcept { std::free(p); }
void operator delete[](void *p, std::size_t) noexcept { std::free(p); }
void operator delete(void *p, const std::nothrow_t &) noexcept { std::free(p); }
void operator delete[](void *p, const std::nothrow_t &) noexcept { std::free(p); }

static const int64_t N = 50000;

struct Rec {
  int64_t id;
  std::string name, tag1, tag2;
  int64_t score;
  bool active;
};

int main() {
  jw_phases ph;
  jw_start(&ph);

  std::vector<Rec> recs;
  for (int64_t i = 0; i < N; i++) {
    recs.push_back(Rec{i, "user_" + std::to_string(i), "t" + std::to_string(i % 7),
                       "t" + std::to_string(i % 11), (i * 37) % 1000, i % 2 == 0});
  }
  jw_mark(&ph);

  simdjson::builder::string_builder sb;
  sb.start_array();
  bool first = true;
  for (const Rec &r : recs) {
    if (!first) sb.append_comma();
    first = false;
    sb.start_object();
    sb.append_key_value("id", r.id);
    sb.append_comma();
    sb.append_key_value("name", std::string_view(r.name));
    sb.append_comma();
    sb.escape_and_append_with_quotes("tags");
    sb.append_colon();
    sb.start_array();
    sb.escape_and_append_with_quotes(std::string_view(r.tag1));
    sb.append_comma();
    sb.escape_and_append_with_quotes(std::string_view(r.tag2));
    sb.end_array();
    sb.append_comma();
    sb.escape_and_append_with_quotes("meta");
    sb.append_colon();
    sb.start_object();
    sb.append_key_value("score", r.score);
    sb.append_comma();
    sb.append_key_value("active", r.active);
    sb.end_object();
    sb.end_object();
  }
  sb.end_array();
  std::string_view text;
  if (sb.view().get(text)) { std::fprintf(stderr, "builder failed\n"); return 1; }
  jw_mark(&ph);

  simdjson::padded_string padded(text);
  int64_t sum = 0;
#ifndef JW_ONDEMAND
  simdjson::dom::parser parser;
  simdjson::dom::element doc;
  if (parser.parse(padded).get(doc)) { std::fprintf(stderr, "parse failed\n"); return 1; }
  jw_mark(&ph);
  simdjson::dom::array arr;
  if (doc.get_array().get(arr)) return 1;
  for (simdjson::dom::element r : arr) {
    int64_t s = 0;
    if (r["meta"]["score"].get_int64().get(s)) s = 0;
    sum += s;
  }
  jw_mark(&ph);
#else
  simdjson::ondemand::parser parser;
  simdjson::ondemand::document doc;
  if (parser.iterate(padded).get(doc)) { std::fprintf(stderr, "parse failed\n"); return 1; }
  simdjson::ondemand::array arr;
  if (doc.get_array().get(arr)) return 1;
  for (auto r : arr) {
    int64_t s = 0;
    if (r["meta"]["score"].get_int64().get(s)) s = 0;
    sum += s;
  }
  jw_mark(&ph);
  jw_mark(&ph);
#endif

  std::printf("%lld %zu\n", (long long)sum, text.size());
  jw_report(&ph);
  std::printf("IMPL %s\n", simdjson::get_active_implementation()->name().c_str());
  return 0;
}
