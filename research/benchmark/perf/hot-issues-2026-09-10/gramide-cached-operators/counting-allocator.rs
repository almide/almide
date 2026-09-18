static PROBE_ALLOCS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROBE_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PROBE_SMALL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct ProbeAllocator;
unsafe impl std::alloc::GlobalAlloc for ProbeAllocator {
 unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
  PROBE_ALLOCS.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
  PROBE_BYTES.fetch_add(l.size() as u64,std::sync::atomic::Ordering::Relaxed);
  if l.size()<=32 { PROBE_SMALL.fetch_add(1,std::sync::atomic::Ordering::Relaxed); }
  std::alloc::System.alloc(l)
 }
 unsafe fn dealloc(&self,p:*mut u8,l:std::alloc::Layout) {std::alloc::System.dealloc(p,l)}
 unsafe fn realloc(&self,p:*mut u8,l:std::alloc::Layout,n:usize)->*mut u8 {
  PROBE_ALLOCS.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
  PROBE_BYTES.fetch_add(n as u64,std::sync::atomic::Ordering::Relaxed);
  if n<=32 { PROBE_SMALL.fetch_add(1,std::sync::atomic::Ordering::Relaxed); }
  std::alloc::System.realloc(p,l,n)
 }
}
#[global_allocator] static PROBE_ALLOCATOR: ProbeAllocator=ProbeAllocator;
fn main(){probe_main(); eprintln!("allocations={} requested_bytes={} small_allocations={}",PROBE_ALLOCS.load(std::sync::atomic::Ordering::Relaxed),PROBE_BYTES.load(std::sync::atomic::Ordering::Relaxed),PROBE_SMALL.load(std::sync::atomic::Ordering::Relaxed));}
