/* Shared by the C/C++ benches: per-phase wall time (CLOCK_MONOTONIC) and
   per-phase allocation counts (each bench increments jw_allocs/jw_reallocs
   from its allocator hook). */
#ifndef JW_PHASES_H
#define JW_PHASES_H
#include <stdint.h>
#include <stdio.h>
#include <time.h>

static unsigned long long jw_allocs = 0, jw_reallocs = 0;

static inline uint64_t jw_now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

typedef struct {
  uint64_t last;
  unsigned long long la, lr;
  int n;
  uint64_t ns[4];
  unsigned long long a[4], r[4];
} jw_phases;

static inline void jw_start(jw_phases *p) {
  p->n = 0; p->la = jw_allocs; p->lr = jw_reallocs; p->last = jw_now();
}
static inline void jw_mark(jw_phases *p) {
  uint64_t t = jw_now();
  p->ns[p->n] = t - p->last;
  p->a[p->n] = jw_allocs - p->la;
  p->r[p->n] = jw_reallocs - p->lr;
  p->n++;
  p->la = jw_allocs; p->lr = jw_reallocs; p->last = jw_now();
}
static inline void jw_report(const jw_phases *p) {
  printf("PHASES build=%llu stringify=%llu parse=%llu sum=%llu\n",
         (unsigned long long)p->ns[0], (unsigned long long)p->ns[1],
         (unsigned long long)p->ns[2], (unsigned long long)p->ns[3]);
  printf("ALLOCS build=%llu/%llu stringify=%llu/%llu parse=%llu/%llu sum=%llu/%llu\n",
         p->a[0], p->r[0], p->a[1], p->r[1], p->a[2], p->r[2], p->a[3], p->r[3]);
}
#endif
