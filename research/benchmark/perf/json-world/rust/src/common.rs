// Shared by every bin: per-phase wall time (Instant) and, under the `count`
// feature, per-phase allocation counts from a counting global allocator.
use std::time::Instant;

#[cfg(feature = "count")]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    pub static ALLOCS: AtomicU64 = AtomicU64::new(0);
    pub static REALLOCS: AtomicU64 = AtomicU64::new(0);
    pub struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            ALLOCS.fetch_add(1, Relaxed);
            System.alloc(l)
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            ALLOCS.fetch_add(1, Relaxed);
            System.alloc_zeroed(l)
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            REALLOCS.fetch_add(1, Relaxed);
            System.realloc(p, l, n)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            System.dealloc(p, l)
        }
    }
    #[global_allocator]
    static GLOBAL: Counting = Counting;
}

fn counts() -> (u64, u64) {
    #[cfg(feature = "count")]
    {
        use std::sync::atomic::Ordering::Relaxed;
        return (counting::ALLOCS.load(Relaxed), counting::REALLOCS.load(Relaxed));
    }
    #[allow(unreachable_code)]
    (0, 0)
}

pub struct Phases {
    last: Instant,
    last_counts: (u64, u64),
    ns: Vec<u128>,
    allocs: Vec<(u64, u64)>,
}

impl Phases {
    pub fn start() -> Phases {
        Phases { last: Instant::now(), last_counts: counts(), ns: Vec::new(), allocs: Vec::new() }
    }
    pub fn mark(&mut self) {
        let now = Instant::now();
        let c = counts();
        self.ns.push(now.duration_since(self.last).as_nanos());
        self.allocs.push((c.0 - self.last_counts.0, c.1 - self.last_counts.1));
        self.last = Instant::now();
        self.last_counts = counts();
    }
    pub fn report(&self) {
        const NAMES: [&str; 4] = ["build", "stringify", "parse", "sum"];
        let p: Vec<String> = NAMES.iter().zip(&self.ns).map(|(n, v)| format!("{n}={v}")).collect();
        println!("PHASES {}", p.join(" "));
        if cfg!(feature = "count") {
            let a: Vec<String> =
                NAMES.iter().zip(&self.allocs).map(|(n, (a, r))| format!("{n}={a}/{r}")).collect();
            println!("ALLOCS {}", a.join(" "));
        }
    }
}
