import re,os
files=['control.rs','tail.rs','tail_b.rs','tail_c.rs','binds_p2_b.rs','binds_p2_c.rs','binds_p2.rs']
for f in files:
    p='crates/almide-mir/src/lower/'+f
    s=open(p).read()
    n=s.count('Err(LowerError::Unsupported(')
    s=s.replace('Err(LowerError::Unsupported(','Err(crate::lower::wall_trace(')
    open(p,'w').write(s); print(p,n)
p='crates/almide-mir/src/lower/mod.rs'
s=open(p).read()
s+='''
pub(crate) fn wall_trace(s: String) -> LowerError {
    if std::env::var_os("WALLTRACE").is_some() {
        eprintln!("WALLTRACE {s}");
        for l in format!("{}", std::backtrace::Backtrace::force_capture()).lines() {
            if l.contains("almide_mir::") { eprintln!("  {}", l.trim()); }
        }
    }
    LowerError::Unsupported(s)
}
'''
open(p,'w').write(s)
