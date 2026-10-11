// Counts the bytes passed to memcpy / memmove / memset and writes the total
// to $MEMSHIM_OUT at exit. Behaviour is unchanged: each call forwards to libc.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static unsigned long long moved;
static void *(*real_memcpy)(void *, const void *, size_t);
static void *(*real_memmove)(void *, const void *, size_t);
static void *(*real_memset)(void *, int, size_t);
__attribute__((constructor)) static void init(void) {
  real_memcpy = dlsym(RTLD_NEXT, "memcpy");
  real_memmove = dlsym(RTLD_NEXT, "memmove");
  real_memset = dlsym(RTLD_NEXT, "memset");
}
__attribute__((destructor)) static void fini(void) {
  const char *p = getenv("MEMSHIM_OUT");
  if (!p) return;
  FILE *f = fopen(p, "w");
  if (f) { fprintf(f, "%llu\n", moved); fclose(f); }
}
void *memcpy(void *d, const void *s, size_t n) { __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED); return real_memcpy(d, s, n); }
void *memmove(void *d, const void *s, size_t n) { __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED); return real_memmove(d, s, n); }
void *memset(void *d, int c, size_t n) { __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED); return real_memset(d, c, n); }
