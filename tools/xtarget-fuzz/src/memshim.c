// Counts the bytes passed to memcpy / memmove / memset and writes the total
// to $MEMSHIM_OUT at exit.
//
// The copies themselves are plain loops, not glibc's. glibc picks a strategy
// by length (vector loop, `rep movsb`, non-temporal stores), and `rep movsb`
// retires as one instruction whatever it copies, so the instruction count of
// a linear copy rose in steps where the strategy changed: native
// bytes.insert's call shape read 4.89e5 / 7.31e5 / 1.37e6 / 2.17e6 / 4.09e6
// instructions at n = 20000..320000 (rms 6.3% off linear) while its bytes
// moved fit n exactly (GCE c4-standard-4, 2026-10-14). A loop costs the same
// per byte at every length, so both meters see the same linear work. The
// shim is built with -fno-tree-loop-distribute-patterns so the compiler does
// not turn these loops back into calls to the functions they replace, and
// without <string.h>, whose fortify wrappers would redefine them.
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
static unsigned long long moved;
__attribute__((destructor)) static void fini(void) {
  const char *p = getenv("MEMSHIM_OUT");
  if (!p) return;
  FILE *f = fopen(p, "w");
  if (f) { fprintf(f, "%llu\n", moved); fclose(f); }
}
// Both directions run the same loop shape — one load, one store and one
// counting step to zero per byte — so they retire the same instructions per
// byte. A memmove's direction follows where the allocator put its two
// blocks, which can change with n: an up-counting forward loop (compare
// against n) and a down-counting backward one (flags from the decrement)
// differed by one instruction per byte, and wasm env.set's series read 5
// then 4 instructions per byte when its log moved from below the value to
// above it (GCE c4-standard-4, 2026-10-17).
#include <stdint.h>
static void copy_fwd(unsigned char *d, const unsigned char *s, size_t n) {
  d += n;
  s += n;
  for (intptr_t i = -(intptr_t)n; i != 0; i++) d[i] = s[i];
}
static void copy_bwd(unsigned char *d, const unsigned char *s, size_t n) {
  for (intptr_t i = (intptr_t)n; i != 0; i--) d[i - 1] = s[i - 1];
}
void *memcpy(void *d, const void *s, size_t n) {
  __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED);
  copy_fwd(d, s, n);
  return d;
}
void *memmove(void *d, const void *s, size_t n) {
  __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED);
  if ((unsigned char *)d < (const unsigned char *)s) copy_fwd(d, s, n); else copy_bwd(d, s, n);
  return d;
}
void *memset(void *d, int c, size_t n) {
  __atomic_fetch_add(&moved, n, __ATOMIC_RELAXED);
  unsigned char *p = d;
  for (size_t i = 0; i < n; i++) p[i] = (unsigned char)c;
  return d;
}
