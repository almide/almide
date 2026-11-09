/* The corpus row for yyjson: parse corpus.json K times into an immutable doc
   (freed before the next), write one doc K times. Same two output lines as
   corpus.almd. */
#include <stdio.h>
#include <stdlib.h>
#include "phases.h"
#include "yyjson.h"
#define K 20
int main(void) {
  FILE *f = fopen("corpus.json", "rb");
  if (!f) return 2;
  fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  char *text = malloc((size_t)n);
  if (fread(text, 1, (size_t)n, f) != (size_t)n) return 2;
  fclose(f);
  yyjson_doc *doc = yyjson_read(text, (size_t)n, 0);
  uint64_t t0 = jw_now();
  long parsed = 0;
  for (int i = 0; i < K; i++) {
    yyjson_doc *d = yyjson_read(text, (size_t)n, 0);
    parsed += d != NULL;
    yyjson_doc_free(d);
  }
  uint64_t t1 = jw_now();
  size_t written = 0;
  for (int i = 0; i < K; i++) {
    size_t len = 0;
    char *out = yyjson_write(doc, 0, &len);
    written += len;
    free(out);
  }
  uint64_t t2 = jw_now();
  printf("%ld %zu\n", parsed, written / K);
  printf("CORPUS parse=%llu stringify=%llu\n", (unsigned long long)((t1 - t0) / K), (unsigned long long)((t2 - t1) / K));
  return 0;
}
