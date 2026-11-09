# Restore the pre-#4614 walk (whole tree per object) but keep the step counter.
import subprocess
p='crates/almide-wasm/src/witness_paths.rs'
subprocess.run(['git','checkout','1b2357886','--',p],check=True)
s=open(p).read()
s=s.replace('''const PATH_CAP: usize = 64;
''','''const PATH_CAP: usize = 64;
static STEPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn bump() { STEPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
pub(crate) fn steps() -> u64 { STEPS.load(std::sync::atomic::Ordering::Relaxed) }
''',1)
s=s.replace('''            if p.ended {
                next.push(p);
                continue;
            }
            match node {''','''            if p.ended {
                next.push(p);
                continue;
            }
            bump();
            match node {''',1)
s=s.replace('''                    loops.push((body.as_slice(), carried.as_slice(), p.clone()));''','''                    bump();
                    loops.push((body.as_slice(), carried.as_slice(), p.clone()));''',1)
open(p,'w').write(s)
