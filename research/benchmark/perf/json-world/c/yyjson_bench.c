/* yyjson: mutable doc to build and write, immutable doc to read.
   Allocation counts come from a counting yyjson_alc (yyjson allocates every
   doc, pool and output buffer through it). */
#include <stdlib.h>
#include <string.h>
#include "phases.h"
#include "yyjson.h"

#define N 50000

static void *c_malloc(void *ctx, size_t size) { (void)ctx; jw_allocs++; return malloc(size); }
static void *c_realloc(void *ctx, void *ptr, size_t old, size_t size) {
  (void)ctx; (void)old; jw_reallocs++; return realloc(ptr, size);
}
static void c_free(void *ctx, void *ptr) { (void)ctx; free(ptr); }
static const yyjson_alc ALC = {c_malloc, c_realloc, c_free, NULL};

int main(void) {
  jw_phases ph;
  jw_start(&ph);

  yyjson_mut_doc *doc = yyjson_mut_doc_new(&ALC);
  yyjson_mut_val *root = yyjson_mut_arr(doc);
  yyjson_mut_doc_set_root(doc, root);
  char buf[32];
  for (int64_t i = 0; i < N; i++) {
    yyjson_mut_val *rec = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_int(doc, rec, "id", i);
    int len = snprintf(buf, sizeof buf, "user_%lld", (long long)i);
    yyjson_mut_obj_add_strncpy(doc, rec, "name", buf, (size_t)len);
    yyjson_mut_val *tags = yyjson_mut_arr(doc);
    len = snprintf(buf, sizeof buf, "t%lld", (long long)(i % 7));
    yyjson_mut_arr_add_strncpy(doc, tags, buf, (size_t)len);
    len = snprintf(buf, sizeof buf, "t%lld", (long long)(i % 11));
    yyjson_mut_arr_add_strncpy(doc, tags, buf, (size_t)len);
    yyjson_mut_obj_add_val(doc, rec, "tags", tags);
    yyjson_mut_val *meta = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_int(doc, meta, "score", (i * 37) % 1000);
    yyjson_mut_obj_add_bool(doc, meta, "active", i % 2 == 0);
    yyjson_mut_obj_add_val(doc, rec, "meta", meta);
    yyjson_mut_arr_append(root, rec);
  }
  jw_mark(&ph);

  size_t text_len = 0;
  char *text = yyjson_mut_write_opts(doc, 0, &ALC, &text_len, NULL);
  jw_mark(&ph);

  yyjson_doc *back = yyjson_read_opts(text, text_len, 0, &ALC, NULL);
  jw_mark(&ph);

  int64_t sum = 0;
  yyjson_val *arr = yyjson_doc_get_root(back);
  size_t idx, max;
  yyjson_val *r;
  yyjson_arr_foreach(arr, idx, max, r) {
    sum += yyjson_get_int(yyjson_obj_get(yyjson_obj_get(r, "meta"), "score"));
  }
  jw_mark(&ph);

  printf("%lld %zu\n", (long long)sum, text_len);
  jw_report(&ph);
  return 0;
}
