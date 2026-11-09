import sys,subprocess
v=sys.argv[1]
def sub(p,old,new):
    s=open(p).read(); assert s.count(old)>=1,(p,old); s=s.replace(old,new); open(p,'w').write(s)
if v=='neg4615':
    p='crates/almide-mir/src/fan_native.rs'
    sub(p,"        if self.inline.is_inline_fan(span) { return; }\n","")
    sub(p,"IrExprKind::Fan { exprs } if !self.inline.is_inline_fan(e.span) =>","IrExprKind::Fan { exprs } =>")
elif v=='neg4616':
    p='runtime/rs/src/io.rs'
    sub(p,"pub fn almide_rt_io___write_stderr(data: &Vec<u8>) {\n    almide_stderr_write_bytes(data);","pub fn almide_rt_io___write_stderr(data: &Vec<u8>) {\n    almide_stdout_flush();\n    let _ = std::io::stderr().write_all(data);")
    sub(p,"pub fn almide_rt_io_eprint(s: &str) {\n    almide_stderr_write_bytes(s.as_bytes());","pub fn almide_rt_io_eprint(s: &str) {\n    almide_stdout_flush();\n    let _ = std::io::stderr().write_all(s.as_bytes());")
elif v=='old':
    subprocess.run(['git','checkout','-q','b6307a0d0','--','.'],check=True)
