p='crates/almide-mir/src/lower/mod_p3.rs'
s=open(p).read()
old="    pub(crate) fn lower_body_into(&mut self, body: &IrExpr) -> Result<Option<ValueId>, LowerError> {\n"
assert s.count(old)==1
s=s.replace(old, old+"        if std::env::var_os(\"IRDUMP\").is_some() { eprintln!(\"IRDUMP-BEGIN\\n{:#?}\\nIRDUMP-END\", body); }\n")
open(p,'w').write(s)
