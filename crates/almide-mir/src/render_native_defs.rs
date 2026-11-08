// Generated native aggregate definitions. Included in render_native_rec.
/// The struct definitions, or "" when the program declares none (so every
/// render without a heap record is byte-identical to before).
pub(crate) fn struct_defs() -> String {
    RECS.with(|r| {
        let mut out = String::new();
        for (i, d) in r.borrow().iter().enumerate() {
            let target = alias_target(d);
            if let (true, Some(t)) = (d.alias.is_some() || d.cell_of.is_some(), &target) {
                out.push_str(&format!("type {} = {t};\n\n", struct_name(i as u16)));
                continue;
            }
            if let (Some(_), Some(t)) = (d.map_of, &target) {
                out.push_str(&format!("/// `{}`\ntype {} = {t};\n\n", d.name, struct_name(i as u16)));
                continue;
            }
            if let Some(p) = d.option_of {
                out.push_str(&format!("/// `{}`\ntype {} = Option<{}>;\n\n", d.name, struct_name(i as u16), owned_ty(p)));
                continue;
            }
            if let (Some(_), Some(t)) = (d.set_of, &target) {
                out.push_str(&format!("/// `{}`\ntype {} = {t};\n\n", d.name, struct_name(i as u16)));
                continue;
            }
            if let Some((o, e)) = d.result_of {
                out.push_str(&format!("/// `{}`\ntype {} = Result<{}, {}>;\n\n", d.name, struct_name(i as u16), owned_ty(o), owned_ty(e)));
                continue;
            }
            if let Some(e) = d.list_of {
                out.push_str(&format!("/// list `{}`\ntype {} = Vec<{}>;\n\n", d.name, struct_name(i as u16), owned_ty(e)));
                continue;
            }
            if let Some(cases) = &d.cases {
                out.push_str(&enum_def(i as u16, d, cases));
                continue;
            }
            let eq = if d.eq_safe { ", PartialEq" } else { "" };
            let default = if has_default(NTy::Rec(i as u16)) { ", Default" } else { "" };
            out.push_str(&format!("/// record `{}`\n#[derive(Clone{default}{eq})]\nstruct {} {{\n", d.name, struct_name(i as u16)));
            for (k, f) in d.fields.iter().enumerate() {
                out.push_str(&format!("    f{k}: {},\n", owned_ty(*f)));
            }
            out.push_str("}\n\n");
        }
        out
    })
}

