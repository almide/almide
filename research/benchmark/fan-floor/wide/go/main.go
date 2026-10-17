// fan floor, wide probes: usage  wide <seq|par|pool|errgroup> <size|skew|heap|fail> <n> <iters> <reps>
// Every implementation runs every element and keeps the lowest-index error
// (Almide's fan.map rule), so the work and the error policy are the same.
package main

import (
	"fmt"
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"

	"golang.org/x/sync/errgroup"
)

const P = 1000003

func work(s, i, iters int64) int64 {
	acc := s + i
	for j := int64(0); j < iters; j++ {
		acc = (acc*31 + j) % P
	}
	return acc
}

// elem returns (value, error length, failed).
func elem(kind string, s, i, iters int64) (int64, bool) {
	switch kind {
	case "size":
		return work(s, i, iters), true
	case "skew":
		if i%10 == 0 {
			return work(s, i, iters*20), true
		}
		return work(s, i, iters), true
	case "heap":
		parts := make([]string, 20)
		for j := int64(0); j < 20; j++ {
			parts[j] = strconv.FormatInt((s+i)*j%1000, 10)
		}
		return int64(len(strings.Join(parts, ","))), true
	default:
		if i%97 == 13 {
			return int64(len(fmt.Sprintf("bad %d", i))), false
		}
		return work(s, i, iters), true
	}
}

type res struct {
	v  int64
	ok bool
}

// pool: W resident workers, woken per offer, drawing chunks from an atomic counter.
type pool struct {
	w     int
	start []chan func(int)
	done  sync.WaitGroup
}

func newPool(w int) *pool {
	p := &pool{w: w, start: make([]chan func(int), w)}
	for k := 0; k < w; k++ {
		p.start[k] = make(chan func(int), 1)
		go func(k int) {
			for f := range p.start[k] {
				f(k)
				p.done.Done()
			}
		}(k)
	}
	return p
}

func (p *pool) run(f func(int)) {
	p.done.Add(p.w)
	for k := 0; k < p.w; k++ {
		p.start[k] <- f
	}
	p.done.Wait()
}

func main() {
	imp, kind := os.Args[1], os.Args[2]
	n, _ := strconv.ParseInt(os.Args[3], 10, 64)
	iters, _ := strconv.ParseInt(os.Args[4], 10, 64)
	reps, _ := strconv.Atoi(os.Args[5])
	W := runtime.GOMAXPROCS(0)
	var pl *pool
	if imp == "pool" {
		pl = newPool(W)
	}
	s := int64(1)
	out := make([]res, n)
	for r := 0; r < reps; r++ {
		s0 := s
		one := func(i int64) { v, ok := elem(kind, s0, i, iters); out[i] = res{v, ok} }
		switch imp {
		case "seq":
			for i := int64(0); i < n; i++ {
				one(i)
			}
		case "par": // a goroutine per element
			var wg sync.WaitGroup
			wg.Add(int(n))
			for i := int64(0); i < n; i++ {
				go func(i int64) { one(i); wg.Done() }(i)
			}
			wg.Wait()
		case "pool": // resident workers, chunks of n / (4W)
			chunk := n / int64(4*W)
			if chunk < 1 {
				chunk = 1
			}
			var next int64
			pl.run(func(int) {
				for {
					lo := atomic.AddInt64(&next, chunk) - chunk
					if lo >= n {
						return
					}
					hi := min(lo+chunk, n)
					for i := lo; i < hi; i++ {
						one(i)
					}
				}
			})
		default: // errgroup with SetLimit(W): per-index errors, no cancellation
			var g errgroup.Group
			g.SetLimit(W)
			for i := int64(0); i < n; i++ {
				i := i
				g.Go(func() error { one(i); return nil })
			}
			g.Wait()
		}
		var sum int64
		failed := false
		for _, x := range out {
			if !x.ok {
				s = (s0 + x.v) % P
				failed = true
				break
			}
			sum = (sum + x.v) % P
		}
		if !failed {
			s = (s0 + sum) % P
		}
	}
	fmt.Println(s)
}
