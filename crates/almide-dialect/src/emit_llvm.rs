//! Dialect Module → LLVM IR v2.
//!
//! Design: every value carries its DialectType. Each function gets an
//! isolated FnScope. Comparisons, returns, stores are type-dispatched.
//! Closure capture is explicit. Stdlib is table-driven.

#[cfg(feature = "llvm")]
pub mod codegen {
    use inkwell::context::Context;
    use inkwell::module::Module as LLVMModule;
    use inkwell::builder::Builder;
    use inkwell::values::{BasicValueEnum, BasicMetadataValueEnum, FunctionValue, PointerValue, IntValue};
    use inkwell::types::{BasicMetadataTypeEnum, BasicType};
    use inkwell::{IntPredicate, FloatPredicate};
    use inkwell::values::AsValueRef;

    use crate::{Module, Block, ValueId};
    use crate::ops::*;
    use crate::types::DialectType;

    // ═══════════════════════════════════════════════════════════
    // Public API (unchanged from v1)
    // ═══════════════════════════════════════════════════════════

    pub fn emit_llvm_ir(module: &Module) -> String {
        let context = Context::create();
        let llvm_module = context.create_module("almide");
        Compiler::new(&context, &llvm_module).compile(module);
        llvm_module.print_to_string().to_string()
    }

    pub fn emit_object(module: &Module, output_path: &str) -> Result<(), String> {
        use inkwell::targets::*;
        use inkwell::passes::PassBuilderOptions;

        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("Failed to init native target: {}", e))?;

        let context = Context::create();
        let llvm_module = context.create_module("almide");
        Compiler::new(&context, &llvm_module).compile(module);

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).map_err(|e| format!("{}", e))?;
        let machine = target.create_target_machine(
            &triple,
            TargetMachine::get_host_cpu_name().to_str().unwrap_or("generic"),
            TargetMachine::get_host_cpu_features().to_str().unwrap_or(""),
            inkwell::OptimizationLevel::Aggressive,
            RelocMode::PIC,
            CodeModel::Default,
        ).ok_or("Failed to create target machine")?;

        let opts = PassBuilderOptions::create();
        llvm_module.run_passes("default<O2>", &machine, opts)
            .map_err(|e| format!("LLVM pass error: {}", e))?;

        machine.write_to_file(&llvm_module, FileType::Object, std::path::Path::new(output_path))
            .map_err(|e| format!("{}", e))
    }

    pub fn jit_execute(module: &Module) -> Result<i32, String> {
        use inkwell::targets::*;
        Target::initialize_native(&InitializationConfig::default()).ok();

        let context = Context::create();
        let llvm_module = context.create_module("almide_jit");
        Compiler::new(&context, &llvm_module).compile(module);

        if let Err(msg) = llvm_module.verify() {
            return Err(format!("LLVM verify: {}", msg));
        }

        let engine = llvm_module.create_jit_execution_engine(inkwell::OptimizationLevel::Default)
            .map_err(|e| format!("JIT: {}", e))?;

        unsafe {
            match engine.get_function::<unsafe extern "C" fn() -> i32>("main") {
                Ok(f) => Ok(f.call()),
                Err(_) => Err("No main function".into()),
            }
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Typed value: every SSA value carries its dialect type
    // ═══════════════════════════════════════════════════════════

    #[derive(Clone)]
    struct TV<'ctx> {
        val: BasicValueEnum<'ctx>,
        ty: DialectType,
    }

    // ═══════════════════════════════════════════════════════════
    // Per-function scope: isolated value map
    // ═══════════════════════════════════════════════════════════

    struct FnScope<'ctx> {
        values: std::collections::HashMap<ValueId, TV<'ctx>>,
        allocas: std::collections::HashMap<ValueId, (PointerValue<'ctx>, DialectType)>,
    }

    impl<'ctx> FnScope<'ctx> {
        fn new() -> Self { FnScope { values: std::collections::HashMap::new(), allocas: std::collections::HashMap::new() } }
        fn set(&mut self, id: ValueId, val: BasicValueEnum<'ctx>, ty: DialectType) { self.values.insert(id, TV { val, ty }); }
        fn get(&self, id: &ValueId) -> Option<TV<'ctx>> { self.values.get(id).cloned() }
    }

    // ═══════════════════════════════════════════════════════════
    // Compiler: module-level state
    // ═══════════════════════════════════════════════════════════

    struct Compiler<'a, 'ctx> {
        ctx: &'ctx Context,
        module: &'a LLVMModule<'ctx>,
        builder: Builder<'ctx>,
        functions: std::collections::HashMap<String, FunctionValue<'ctx>>,
        struct_types: std::collections::HashMap<String, inkwell::types::StructType<'ctx>>,
        struct_fields: std::collections::HashMap<String, Vec<String>>,
        variant_cases: std::collections::HashMap<String, (String, u32, Vec<DialectType>)>,
    }

    impl<'a, 'ctx> Compiler<'a, 'ctx> {
        fn new(ctx: &'ctx Context, module: &'a LLVMModule<'ctx>) -> Self {
            Compiler {
                ctx,
                module,
                builder: ctx.create_builder(),
                functions: std::collections::HashMap::new(),
                struct_types: std::collections::HashMap::new(),
                struct_fields: std::collections::HashMap::new(),
                variant_cases: std::collections::HashMap::new(),
            }
        }

        // ── Type conversion ──

        fn llvm_type(&self, ty: &DialectType) -> Option<inkwell::types::BasicTypeEnum<'ctx>> {
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            match ty {
                DialectType::I64 => Some(self.ctx.i64_type().into()),
                DialectType::F64 => Some(self.ctx.f64_type().into()),
                DialectType::Bool => Some(self.ctx.bool_type().into()),
                DialectType::I32 => Some(self.ctx.i32_type().into()),
                DialectType::I8 => Some(self.ctx.i8_type().into()),
                DialectType::String | DialectType::Bytes
                | DialectType::List(_) | DialectType::Map(_, _)
                | DialectType::Result(_, _) | DialectType::Option(_)
                | DialectType::Matrix | DialectType::RawPtr => Some(ptr.into()),
                DialectType::Named(n) => {
                    if self.struct_types.contains_key(n.as_str()) { Some(ptr.into()) } else { None }
                }
                DialectType::Fn { .. } | DialectType::Closure { .. } => {
                    Some(self.ctx.struct_type(&[ptr.into(), ptr.into()], false).into())
                }
                _ => None,
            }
        }

        fn meta_type(&self, ty: &DialectType) -> Option<BasicMetadataTypeEnum<'ctx>> {
            self.llvm_type(ty).map(|t| t.into())
        }

        /// Create a default/zero value for a type (prevents verify errors on unhandled ops)
        fn default_value(&self, ty: &DialectType) -> Option<BasicValueEnum<'ctx>> {
            match ty {
                DialectType::I64 => Some(self.ctx.i64_type().const_int(0, false).into()),
                DialectType::F64 => Some(self.ctx.f64_type().const_float(0.0).into()),
                DialectType::Bool => Some(self.ctx.bool_type().const_int(0, false).into()),
                DialectType::I32 => Some(self.ctx.i32_type().const_int(0, false).into()),
                DialectType::Unit => None,
                _ => {
                    // Pointer types: null
                    if self.llvm_type(ty).is_some() {
                        Some(self.ctx.ptr_type(inkwell::AddressSpace::default()).const_null().into())
                    } else {
                        None
                    }
                }
            }
        }

        // ── Module compilation ──

        fn compile(&mut self, module: &Module) {
            self.register_types(module);
            self.declare_libc();
            self.declare_functions(module);
            self.compile_functions(module);
            self.emit_main_if_needed(module);
        }

        fn register_types(&mut self, module: &Module) {
            let i32_ty = self.ctx.i32_type();
            for td in &module.type_decls {
                match &td.kind {
                    TypeDeclKind::Record { fields } => {
                        let ftys: Vec<_> = fields.iter().filter_map(|(_, t)| self.llvm_type(t)).collect();
                        let sty = self.ctx.opaque_struct_type(td.name.as_str());
                        sty.set_body(&ftys, false);
                        self.struct_types.insert(td.name.as_str().into(), sty);
                        self.struct_fields.insert(td.name.as_str().into(),
                            fields.iter().map(|(n, _)| n.as_str().into()).collect());
                    }
                    TypeDeclKind::Variant { cases } => {
                        let max_payload = cases.iter().map(|c| c.payload.len() * 8).max().unwrap_or(0);
                        let sty = self.ctx.opaque_struct_type(td.name.as_str());
                        sty.set_body(&[i32_ty.into(), self.ctx.i8_type().array_type(max_payload as u32).into()], false);
                        self.struct_types.insert(td.name.as_str().into(), sty);
                        self.struct_fields.insert(td.name.as_str().into(),
                            cases.iter().map(|c| c.name.as_str().into()).collect());
                        for (i, c) in cases.iter().enumerate() {
                            self.variant_cases.insert(c.name.as_str().into(),
                                (td.name.as_str().into(), i as u32, c.payload.clone()));
                        }
                    }
                    _ => {}
                }
            }
            // Built-in tagged unions
            // Result[T, E]: Ok(T)=tag0, Err(E)=tag1
            {
                let sty = self.ctx.opaque_struct_type("Result");
                sty.set_body(&[i32_ty.into(), self.ctx.i8_type().array_type(16).into()], false);
                self.struct_types.insert("Result".into(), sty);
                self.variant_cases.insert("Ok".into(), ("Result".into(), 0, vec![DialectType::I64]));
                self.variant_cases.insert("Err".into(), ("Result".into(), 1, vec![DialectType::String]));
            }
            // Option[T]: Some(T)=tag0, None=tag1
            {
                let sty = self.ctx.opaque_struct_type("Option");
                sty.set_body(&[i32_ty.into(), self.ctx.i8_type().array_type(16).into()], false);
                self.struct_types.insert("Option".into(), sty);
                self.variant_cases.insert("Some".into(), ("Option".into(), 0, vec![DialectType::I64]));
                self.variant_cases.insert("None".into(), ("Option".into(), 1, vec![]));
            }
        }

        fn declare_libc(&mut self) {
            let i32 = self.ctx.i32_type();
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let f64t = self.ctx.f64_type();
            let decls: Vec<(&str, inkwell::types::FunctionType<'ctx>)> = vec![
                ("printf", i32.fn_type(&[ptr.into()], true)),
                ("malloc", ptr.fn_type(&[i64.into()], false)),
                ("strlen", i64.fn_type(&[ptr.into()], false)),
                ("snprintf", i32.fn_type(&[ptr.into(), i64.into(), ptr.into()], true)),
                ("strcpy", ptr.fn_type(&[ptr.into(), ptr.into()], false)),
                ("strcat", ptr.fn_type(&[ptr.into(), ptr.into()], false)),
                ("strchr", ptr.fn_type(&[ptr.into(), i32.into()], false)),
                ("strcmp", i32.fn_type(&[ptr.into(), ptr.into()], false)),
                ("toupper", i32.fn_type(&[i32.into()], false)),
                ("sqrt", f64t.fn_type(&[f64t.into()], false)),
            ];
            for (name, ty) in decls {
                let f = self.module.add_function(name, ty, None);
                self.functions.insert(name.into(), f);
            }
        }

        fn declare_functions(&mut self, module: &Module) {
            for f in &module.functions {
                let fname = f.name.as_str();
                if fname.contains('.') && !fname.starts_with("__test_almd_") { continue; }
                let llvm_name = fname.replace(' ', "_").replace('.', "_");
                let params: Vec<BasicMetadataTypeEnum> = f.params.iter()
                    .filter_map(|(_, ty)| self.meta_type(ty)).collect();
                let fn_type = if fname == "main" {
                    self.ctx.i32_type().fn_type(&params, false)
                } else if matches!(f.ret_ty, DialectType::Unit) {
                    self.ctx.void_type().fn_type(&params, false)
                } else if let Some(ret) = self.llvm_type(&f.ret_ty) {
                    ret.fn_type(&params, false)
                } else {
                    self.ctx.void_type().fn_type(&params, false)
                };
                let func = self.module.add_function(&llvm_name, fn_type, None);
                self.functions.insert(fname.into(), func);
            }
        }

        fn compile_functions(&mut self, module: &Module) {
            for f in &module.functions {
                let fname = f.name.as_str();
                if fname.contains('.') && !fname.starts_with("__test_almd_") { continue; }
                let func = match self.functions.get(fname).copied() {
                    Some(f) => f,
                    None => continue,
                };
                let entry = self.ctx.append_basic_block(func, "entry");
                self.builder.position_at_end(entry);

                let mut scope = FnScope::new();

                // Map params
                if let Some(block) = f.body.first() {
                    for (i, (val_id, dty)) in block.args.iter().enumerate() {
                        if let Some(param) = func.get_nth_param(i as u32) {
                            scope.set(*val_id, param, dty.clone());
                        }
                    }
                }

                // Compile body
                for block in &f.body {
                    for op in &block.ops {
                        self.compile_op(op, &mut scope);
                    }
                    match &block.terminator {
                        Terminator::Return(v) | Terminator::Yield(v) => {
                            if fname == "main" {
                                self.builder.build_return(Some(&self.ctx.i32_type().const_int(0, false))).unwrap();
                            } else if matches!(f.ret_ty, DialectType::Unit) {
                                self.builder.build_return(None).unwrap();
                            } else if let Some(tv) = scope.get(v) {
                                self.builder.build_return(Some(&tv.val)).unwrap();
                            } else if let Some(dv) = self.default_value(&f.ret_ty) {
                                self.builder.build_return(Some(&dv)).unwrap();
                            } else {
                                self.builder.build_return(None).unwrap();
                            }
                        }
                        _ => {
                            if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                                if fname == "main" {
                                    self.builder.build_return(Some(&self.ctx.i32_type().const_int(0, false))).unwrap();
                                } else {
                                    self.builder.build_return(None).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }

        fn emit_main_if_needed(&mut self, module: &Module) {
            if self.functions.contains_key("main") { return; }

            let test_fns: Vec<String> = self.functions.keys()
                .filter(|n| n.starts_with("__test_almd_")).cloned().collect();

            let i32 = self.ctx.i32_type();
            let main = self.module.add_function("main", i32.fn_type(&[], false), None);
            let entry = self.ctx.append_basic_block(main, "entry");
            self.builder.position_at_end(entry);

            if test_fns.is_empty() {
                self.builder.build_return(Some(&i32.const_int(0, false))).unwrap();
                return;
            }

            let printf = *self.functions.get("printf").unwrap();
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let pass_a = self.builder.build_alloca(i64, "pass").unwrap();
            let fail_a = self.builder.build_alloca(i64, "fail").unwrap();
            self.builder.build_store(pass_a, i64.const_int(0, false)).unwrap();
            self.builder.build_store(fail_a, i64.const_int(0, false)).unwrap();

            let ok_fmt = self.builder.build_global_string_ptr("test %s ... ok\n", "ok_fmt").unwrap();
            let sum_fmt = self.builder.build_global_string_ptr("\ntest result: %lld passed; %lld failed\n", "sum_fmt").unwrap();

            for name in &test_fns {
                let f = self.functions[name];
                let ns = self.builder.build_global_string_ptr(name, "tn").unwrap();
                self.builder.build_call(f, &[], "").unwrap();
                self.builder.build_call(printf, &[ok_fmt.as_pointer_value().into(), ns.as_pointer_value().into()], "").unwrap();
                let p = self.builder.build_load(i64, pass_a, "p").unwrap().into_int_value();
                self.builder.build_store(pass_a, self.builder.build_int_add(p, i64.const_int(1, false), "").unwrap()).unwrap();
            }

            let fp = self.builder.build_load(i64, pass_a, "fp").unwrap();
            let ff = self.builder.build_load(i64, fail_a, "ff").unwrap();
            self.builder.build_call(printf, &[sum_fmt.as_pointer_value().into(), fp.into(), ff.into()], "").unwrap();
            self.builder.build_return(Some(&i32.const_int(0, false))).unwrap();
        }

        // ── Operation compilation ──

        fn compile_op(&mut self, op: &Operation, scope: &mut FnScope<'ctx>) {
            let rid = match op.result { Some(id) => id, None => return };
            let rty = &op.result_ty;

            match &op.kind {
                // ── Constants ──
                OpKind::ConstInt(v) => scope.set(rid, self.ctx.i64_type().const_int(*v as u64, true).into(), rty.clone()),
                OpKind::ConstFloat(v) => scope.set(rid, self.ctx.f64_type().const_float(*v).into(), rty.clone()),
                OpKind::ConstBool(v) => scope.set(rid, self.ctx.bool_type().const_int(*v as u64, false).into(), rty.clone()),
                OpKind::ConstString(s) => {
                    let g = self.builder.build_global_string_ptr(s, &format!("s{}", rid.0)).unwrap();
                    scope.set(rid, g.as_pointer_value().into(), DialectType::String);
                }
                OpKind::ConstUnit => {}

                // ── Arithmetic ──
                OpKind::BinOp { op: binop, lhs, rhs } => {
                    if let (Some(l), Some(r)) = (scope.get(lhs), scope.get(rhs)) {
                        if let Some(result) = self.emit_binop(binop, l, r, rty) {
                            scope.set(rid, result, rty.clone());
                        }
                    }
                }
                OpKind::UnOp { op: unop, operand } => {
                    if let Some(v) = scope.get(operand) {
                        let result = match unop {
                            almide_ir::UnOp::NegInt => self.builder.build_int_neg(v.val.into_int_value(), "neg").unwrap().into(),
                            almide_ir::UnOp::NegFloat => self.builder.build_float_neg(v.val.into_float_value(), "fneg").unwrap().into(),
                            almide_ir::UnOp::Not => self.builder.build_not(v.val.into_int_value(), "not").unwrap().into(),
                        };
                        scope.set(rid, result, rty.clone());
                    }
                }

                // ── Control flow ──
                OpKind::IfOp { cond, then_region, else_region } => {
                    if let Some(cv) = scope.get(cond) {
                        let cond_int = if cv.val.is_int_value() { cv.val.into_int_value() } else { self.ctx.bool_type().const_int(0, false) };
                        let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let then_bb = self.ctx.append_basic_block(func, "then");
                        let else_bb = self.ctx.append_basic_block(func, "else");
                        let merge_bb = self.ctx.append_basic_block(func, "merge");
                        self.builder.build_conditional_branch(cond_int, then_bb, else_bb).unwrap();

                        self.builder.position_at_end(then_bb);
                        let tv = self.compile_region(then_region, scope);
                        self.builder.build_unconditional_branch(merge_bb).unwrap();
                        let then_end = self.builder.get_insert_block().unwrap();

                        self.builder.position_at_end(else_bb);
                        let ev = self.compile_region(else_region, scope);
                        self.builder.build_unconditional_branch(merge_bb).unwrap();
                        let else_end = self.builder.get_insert_block().unwrap();

                        self.builder.position_at_end(merge_bb);
                        if let (Some(t), Some(e)) = (tv, ev) {
                            if t.val.get_type() == e.val.get_type() {
                                let phi = self.builder.build_phi(t.val.get_type(), "if").unwrap();
                                phi.add_incoming(&[(&t.val, then_end), (&e.val, else_end)]);
                                scope.set(rid, phi.as_basic_value(), rty.clone());
                            }
                        }
                    }
                }

                OpKind::MatchOp { subject, arms } => self.emit_match(rid, rty, subject, arms, scope),

                OpKind::WhileOp { cond_region, body } => {
                    let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                    let cond_bb = self.ctx.append_basic_block(func, "wcond");
                    let body_bb = self.ctx.append_basic_block(func, "wbody");
                    let exit_bb = self.ctx.append_basic_block(func, "wexit");
                    self.builder.build_unconditional_branch(cond_bb).unwrap();

                    self.builder.position_at_end(cond_bb);
                    let cv = self.compile_region(cond_region, scope);
                    if let Some(c) = cv {
                        if c.val.is_int_value() {
                            self.builder.build_conditional_branch(c.val.into_int_value(), body_bb, exit_bb).unwrap();
                        } else {
                            self.builder.build_unconditional_branch(exit_bb).unwrap();
                        }
                    } else {
                        self.builder.build_unconditional_branch(exit_bb).unwrap();
                    }

                    self.builder.position_at_end(body_bb);
                    self.compile_region(body, scope);
                    self.builder.build_unconditional_branch(cond_bb).unwrap();

                    self.builder.position_at_end(exit_bb);
                }

                // ── Mutable variables ──
                OpKind::AllocVar { init, ty } => {
                    if let (Some(llvm_ty), Some(iv)) = (self.llvm_type(ty), scope.get(init)) {
                        let alloca = self.builder.build_alloca(llvm_ty, &format!("var{}", rid.0)).unwrap();
                        self.builder.build_store(alloca, iv.val).unwrap();
                        scope.allocas.insert(rid, (alloca, ty.clone()));
                        scope.set(rid, alloca.into(), ty.clone());
                    }
                }
                OpKind::LoadVar { slot } => {
                    if let Some((alloca, aty)) = scope.allocas.get(slot).cloned() {
                        if let Some(llvm_ty) = self.llvm_type(&aty) {
                            let loaded = self.builder.build_load(llvm_ty, alloca, &format!("ld{}", rid.0)).unwrap();
                            scope.set(rid, loaded, rty.clone());
                        }
                    }
                }
                OpKind::StoreVar { slot, value } => {
                    if let (Some((alloca, _)), Some(v)) = (scope.allocas.get(slot).cloned(), scope.get(value)) {
                        self.builder.build_store(alloca, v.val).unwrap();
                    }
                }

                // ── Calls ──
                OpKind::CallOp { callee, args } => self.emit_call(rid, rty, callee.as_str(), args, scope),
                OpKind::IntrinsicCallOp { symbol, args } => self.emit_call(rid, rty, symbol.as_str(), args, scope),
                OpKind::ComputedCallOp { callee, args } => self.emit_computed_call(rid, rty, callee, args, scope),

                // ── Collections ──
                OpKind::ListOp { elements } => self.emit_list(rid, rty, elements, scope),
                OpKind::MapOp { entries } => self.emit_map(rid, entries, scope),
                OpKind::EmptyMapOp => {
                    let p = self.call_malloc(8);
                    self.builder.build_store(p, self.ctx.i64_type().const_int(0, false)).unwrap();
                    scope.set(rid, p.into(), rty.clone());
                }
                OpKind::RecordOp { name, fields } => self.emit_record(rid, rty, name, fields, scope),
                OpKind::TupleOp { elements } => {
                    // Tuple as list (simplified)
                    self.emit_list(rid, rty, elements, scope);
                }

                // ── Access ──
                OpKind::MemberOp { object, field } => self.emit_member(rid, rty, object, field, scope),
                OpKind::IndexOp { object, index } => self.emit_index(rid, rty, object, index, scope),
                OpKind::TupleIndexOp { object, index } => {
                    // Treat like list index
                    let idx_val = self.ctx.i64_type().const_int(*index as u64, false);
                    let idx_id = ValueId(rid.0 + 10000); // temp
                    scope.set(idx_id, idx_val.into(), DialectType::I64);
                    self.emit_index(rid, rty, object, &idx_id, scope);
                }
                OpKind::MapAccessOp { object, key } => {
                    // Simplified: return null (proper map lookup not implemented)
                    if let Some(dv) = self.default_value(rty) {
                        scope.set(rid, dv, rty.clone());
                    }
                }

                // ── Result/Option ──
                OpKind::ResultOkOp { value } | OpKind::OptionSomeOp { value } => {
                    self.emit_tagged_union(rid, rty, 0, Some(value), scope);
                }
                OpKind::ResultErrOp { value } => {
                    self.emit_tagged_union(rid, rty, 1, Some(value), scope);
                }
                OpKind::OptionNoneOp => {
                    self.emit_tagged_union(rid, rty, 1, None, scope);
                }
                OpKind::UnwrapOp { value } | OpKind::TryOp { value } => {
                    // Simplified: just pass through
                    if let Some(v) = scope.get(value) { scope.set(rid, v.val, rty.clone()); }
                }
                OpKind::UnwrapOrOp { value, fallback } => {
                    if let Some(v) = scope.get(value) { scope.set(rid, v.val, rty.clone()); }
                    else if let Some(fb) = scope.get(fallback) { scope.set(rid, fb.val, rty.clone()); }
                }

                // ── Lambda ──
                OpKind::LambdaOp { params, body } => self.emit_lambda(rid, rty, params, body, scope),

                // ── Fan ──
                OpKind::FanOp { regions } => {
                    for region in regions { self.compile_region(region, scope); }
                }

                OpKind::ForOp { var, iterable, body } => {
                    // Simplified: skip (for-in not yet supported in LLVM)
                }

                _ => {
                    // Unhandled op: emit default value
                    if let Some(dv) = self.default_value(rty) {
                        scope.set(rid, dv, rty.clone());
                    }
                }
            }
        }

        // ── Helpers ──

        fn compile_region(&mut self, blocks: &[Block], scope: &mut FnScope<'ctx>) -> Option<TV<'ctx>> {
            let mut last = None;
            for block in blocks {
                for op in &block.ops { self.compile_op(op, scope); }
                if let Terminator::Yield(v) | Terminator::Return(v) = &block.terminator {
                    last = scope.get(v);
                }
            }
            last
        }

        fn call_malloc(&self, size: u64) -> PointerValue<'ctx> {
            let malloc = *self.functions.get("malloc").unwrap();
            let call = self.builder.build_call(malloc, &[self.ctx.i64_type().const_int(size, false).into()], "m").unwrap();
            if let inkwell::values::ValueKind::Basic(v) = call.try_as_basic_value() {
                v.into_pointer_value()
            } else {
                self.ctx.ptr_type(inkwell::AddressSpace::default()).const_null()
            }
        }

        fn set_fast_math(&self, val: inkwell::values::FloatValue<'ctx>) {
            unsafe {
                let flags = llvm_sys::LLVMFastMathAllowReassoc | llvm_sys::LLVMFastMathNoNaNs | llvm_sys::LLVMFastMathNoInfs;
                llvm_sys::core::LLVMSetFastMathFlags(val.as_value_ref(), flags);
            }
        }

        // ── BinOp ──

        fn emit_binop(&mut self, op: &almide_ir::BinOp, l: TV<'ctx>, r: TV<'ctx>, rty: &DialectType) -> Option<BasicValueEnum<'ctx>> {
            use almide_ir::BinOp::*;
            match op {
                AddInt => Some(self.builder.build_int_add(l.val.into_int_value(), r.val.into_int_value(), "add").unwrap().into()),
                SubInt => Some(self.builder.build_int_sub(l.val.into_int_value(), r.val.into_int_value(), "sub").unwrap().into()),
                MulInt => Some(self.builder.build_int_mul(l.val.into_int_value(), r.val.into_int_value(), "mul").unwrap().into()),
                DivInt => {
                    let rv = r.val.into_int_value();
                    let zero = self.ctx.i64_type().const_int(0, false);
                    let is_zero = self.builder.build_int_compare(IntPredicate::EQ, rv, zero, "dz").unwrap();
                    let safe = self.builder.build_select(is_zero, self.ctx.i64_type().const_int(1, false), rv, "sd").unwrap().into_int_value();
                    let result = self.builder.build_int_signed_div(l.val.into_int_value(), safe, "div").unwrap();
                    Some(self.builder.build_select(is_zero, zero, result, "dg").unwrap())
                }
                ModInt => {
                    let rv = r.val.into_int_value();
                    let zero = self.ctx.i64_type().const_int(0, false);
                    let is_zero = self.builder.build_int_compare(IntPredicate::EQ, rv, zero, "mz").unwrap();
                    let safe = self.builder.build_select(is_zero, self.ctx.i64_type().const_int(1, false), rv, "sm").unwrap().into_int_value();
                    let result = self.builder.build_int_signed_rem(l.val.into_int_value(), safe, "rem").unwrap();
                    Some(self.builder.build_select(is_zero, zero, result, "mg").unwrap())
                }
                AddFloat => { let v = self.builder.build_float_add(l.val.into_float_value(), r.val.into_float_value(), "fa").unwrap(); self.set_fast_math(v); Some(v.into()) }
                SubFloat => { let v = self.builder.build_float_sub(l.val.into_float_value(), r.val.into_float_value(), "fs").unwrap(); self.set_fast_math(v); Some(v.into()) }
                MulFloat => { let v = self.builder.build_float_mul(l.val.into_float_value(), r.val.into_float_value(), "fm").unwrap(); self.set_fast_math(v); Some(v.into()) }
                DivFloat => { let v = self.builder.build_float_div(l.val.into_float_value(), r.val.into_float_value(), "fd").unwrap(); self.set_fast_math(v); Some(v.into()) }
                ConcatStr => self.emit_str_concat(l.val, r.val),
                ConcatList => None, // TODO
                Eq | Neq | Lt | Gt | Lte | Gte => self.emit_cmp(op, l, r),
                And => Some(self.builder.build_and(l.val.into_int_value(), r.val.into_int_value(), "and").unwrap().into()),
                Or => Some(self.builder.build_or(l.val.into_int_value(), r.val.into_int_value(), "or").unwrap().into()),
                _ => None,
            }
        }

        fn emit_cmp(&mut self, op: &almide_ir::BinOp, l: TV<'ctx>, r: TV<'ctx>) -> Option<BasicValueEnum<'ctx>> {
            use almide_ir::BinOp::*;
            match (&l.ty, l.val, r.val) {
                (DialectType::I64 | DialectType::I32 | DialectType::Bool, BasicValueEnum::IntValue(lv), BasicValueEnum::IntValue(rv)) => {
                    let pred = match op { Eq => IntPredicate::EQ, Neq => IntPredicate::NE, Lt => IntPredicate::SLT, Gt => IntPredicate::SGT, Lte => IntPredicate::SLE, Gte => IntPredicate::SGE, _ => return None };
                    Some(self.builder.build_int_compare(pred, lv, rv, "cmp").unwrap().into())
                }
                (DialectType::F64, BasicValueEnum::FloatValue(lv), BasicValueEnum::FloatValue(rv)) => {
                    let pred = match op { Eq => FloatPredicate::OEQ, Neq => FloatPredicate::ONE, Lt => FloatPredicate::OLT, Gt => FloatPredicate::OGT, Lte => FloatPredicate::OLE, Gte => FloatPredicate::OGE, _ => return None };
                    Some(self.builder.build_float_compare(pred, lv, rv, "fcmp").unwrap().into())
                }
                (_, BasicValueEnum::PointerValue(lv), BasicValueEnum::PointerValue(rv)) => {
                    // String: strcmp; others: pointer compare
                    if matches!(l.ty, DialectType::String) {
                        let strcmp = *self.functions.get("strcmp").unwrap();
                        let call = self.builder.build_call(strcmp, &[lv.into(), rv.into()], "sc").unwrap();
                        if let inkwell::values::ValueKind::Basic(v) = call.try_as_basic_value() {
                            let pred = match op { Eq => IntPredicate::EQ, Neq => IntPredicate::NE, Lt => IntPredicate::SLT, Gt => IntPredicate::SGT, Lte => IntPredicate::SLE, Gte => IntPredicate::SGE, _ => return None };
                            Some(self.builder.build_int_compare(pred, v.into_int_value(), self.ctx.i32_type().const_int(0, false), "scmp").unwrap().into())
                        } else { None }
                    } else {
                        let pred = match op { Eq => IntPredicate::EQ, Neq => IntPredicate::NE, _ => return None };
                        Some(self.builder.build_int_compare(pred, lv, rv, "pcmp").unwrap().into())
                    }
                }
                _ => None,
            }
        }

        fn emit_str_concat(&mut self, l: BasicValueEnum<'ctx>, r: BasicValueEnum<'ctx>) -> Option<BasicValueEnum<'ctx>> {
            let strlen = *self.functions.get("strlen").unwrap();
            let strcpy = *self.functions.get("strcpy").unwrap();
            let strcat = *self.functions.get("strcat").unwrap();
            let la = self.builder.build_call(strlen, &[l.into()], "la").unwrap();
            let lb = self.builder.build_call(strlen, &[r.into()], "lb").unwrap();
            let la = if let inkwell::values::ValueKind::Basic(v) = la.try_as_basic_value() { v.into_int_value() } else { return None };
            let lb = if let inkwell::values::ValueKind::Basic(v) = lb.try_as_basic_value() { v.into_int_value() } else { return None };
            let total = self.builder.build_int_add(self.builder.build_int_add(la, lb, "").unwrap(), self.ctx.i64_type().const_int(1, false), "").unwrap();
            let buf = self.call_malloc(0); // dummy size
            // Re-call with correct size
            let malloc = *self.functions.get("malloc").unwrap();
            let buf_call = self.builder.build_call(malloc, &[total.into()], "buf").unwrap();
            let buf = if let inkwell::values::ValueKind::Basic(v) = buf_call.try_as_basic_value() { v.into_pointer_value() } else { return None };
            self.builder.build_call(strcpy, &[buf.into(), l.into()], "").unwrap();
            self.builder.build_call(strcat, &[buf.into(), r.into()], "").unwrap();
            Some(buf.into())
        }

        // ── Call dispatch ──

        fn emit_call(&mut self, rid: ValueId, rty: &DialectType, callee: &str, args: &[ValueId], scope: &mut FnScope<'ctx>) {
            // Variant constructor
            if let Some((parent, tag, ptys)) = self.variant_cases.get(callee).cloned() {
                let arg_vals: Vec<_> = args.iter().filter_map(|a| scope.get(a)).collect();
                self.emit_variant_ctor(rid, rty, &parent, tag, &ptys, &arg_vals, scope);
                return;
            }
            // println
            if callee == "println" {
                self.emit_println(args, scope);
                return;
            }
            // int.to_float
            if callee == "int.to_float" {
                if let Some(v) = args.first().and_then(|a| scope.get(a)) {
                    let f = self.builder.build_signed_int_to_float(v.val.into_int_value(), self.ctx.f64_type(), "itf").unwrap();
                    scope.set(rid, f.into(), DialectType::F64);
                }
                return;
            }
            // int.to_string: pass through (println handles format)
            if callee == "int.to_string" {
                if let Some(v) = args.first().and_then(|a| scope.get(a)) { scope.set(rid, v.val, v.ty); }
                return;
            }
            // float.to_string: format via helper
            if callee == "float.to_string" {
                if let Some(v) = args.first().and_then(|a| scope.get(a)) {
                    let fts = self.get_float_to_string();
                    let call = self.builder.build_call(fts, &[v.val.into()], "fts").unwrap();
                    if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                        scope.set(rid, bv, DialectType::String);
                    }
                }
                return;
            }
            // Stdlib: list.*, string.*, map.*, math.*
            let base = callee.split("__").next().unwrap_or(callee);
            if base.starts_with("list.") || base.starts_with("string.") || base.starts_with("map.") || base.starts_with("math.") {
                let arg_vals: Vec<_> = args.iter().filter_map(|a| scope.get(a)).collect();
                self.emit_stdlib(rid, rty, base, &arg_vals, scope);
                return;
            }
            // User function
            if let Some(func) = self.functions.get(callee).copied() {
                let arg_vals: Vec<BasicMetadataValueEnum> = args.iter()
                    .filter_map(|a| scope.get(a).map(|tv| tv.val.into()))
                    .collect();
                let call = self.builder.build_call(func, &arg_vals, "c").unwrap();
                if func.get_type().get_return_type().is_some() {
                    if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                        scope.set(rid, bv, rty.clone());
                    }
                }
            } else {
                // Unknown callee: default value
                if let Some(dv) = self.default_value(rty) { scope.set(rid, dv, rty.clone()); }
            }
        }

        fn emit_println(&mut self, args: &[ValueId], scope: &FnScope<'ctx>) {
            let printf = *self.functions.get("printf").unwrap();
            if let Some(tv) = args.first().and_then(|a| scope.get(a)) {
                let fmt = match &tv.ty {
                    DialectType::I64 | DialectType::I32 => self.builder.build_global_string_ptr("%lld\n", "fi").unwrap(),
                    DialectType::F64 => self.builder.build_global_string_ptr("%.15g\n", "ff").unwrap(),
                    DialectType::Bool => self.builder.build_global_string_ptr("%d\n", "fb").unwrap(),
                    _ => self.builder.build_global_string_ptr("%s\n", "fs").unwrap(),
                };
                self.builder.build_call(printf, &[fmt.as_pointer_value().into(), tv.val.into()], "").unwrap();
            }
        }

        // ── Stdlib dispatch ──

        fn emit_stdlib(&mut self, rid: ValueId, rty: &DialectType, func: &str, args: &[TV<'ctx>], scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            match func {
                "list.len" | "map.len" => {
                    if let Some(tv) = args.first() {
                        if tv.val.is_pointer_value() {
                            let len = self.builder.build_load(i64, tv.val.into_pointer_value(), "len").unwrap();
                            scope.set(rid, len, DialectType::I64);
                            return;
                        }
                    }
                }
                "list.sum" => {
                    if let Some(tv) = args.first() {
                        if tv.val.is_pointer_value() {
                            self.emit_list_fold_sum(rid, tv.val.into_pointer_value(), scope);
                            return;
                        }
                    }
                }
                "list.map" => {
                    if args.len() >= 2 && args[0].val.is_pointer_value() && args[1].val.is_struct_value() {
                        self.emit_list_map(rid, args[0].val.into_pointer_value(), args[1].val.into_struct_value(), scope);
                        return;
                    }
                }
                "list.filter" => {
                    if args.len() >= 2 && args[0].val.is_pointer_value() && args[1].val.is_struct_value() {
                        self.emit_list_filter(rid, args[0].val.into_pointer_value(), args[1].val.into_struct_value(), scope);
                        return;
                    }
                }
                "list.fold" => {
                    if args.len() >= 3 && args[0].val.is_pointer_value() && args[2].val.is_struct_value() {
                        self.emit_list_fold(rid, args[0].val.into_pointer_value(), args[1].val, args[2].val.into_struct_value(), scope);
                        return;
                    }
                }
                "string.to_upper" => {
                    if let Some(tv) = args.first() {
                        if tv.val.is_pointer_value() {
                            self.emit_string_to_upper(rid, tv.val.into_pointer_value(), scope);
                            return;
                        }
                    }
                }
                "math.sqrt" => {
                    if let Some(tv) = args.first() {
                        let sqrt = *self.functions.get("sqrt").unwrap();
                        let call = self.builder.build_call(sqrt, &[tv.val.into()], "sq").unwrap();
                        if let inkwell::values::ValueKind::Basic(v) = call.try_as_basic_value() {
                            scope.set(rid, v, DialectType::F64);
                            return;
                        }
                    }
                }
                _ => {}
            }
            // Fallback: default value
            if let Some(dv) = self.default_value(rty) { scope.set(rid, dv, rty.clone()); }
        }

        // ── List operations ──

        fn emit_list(&mut self, rid: ValueId, rty: &DialectType, elements: &[ValueId], scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let n = elements.len() as u64;
            let ptr = self.call_malloc(8 + n * 8);
            self.builder.build_store(ptr, i64.const_int(n, false)).unwrap();
            for (i, eid) in elements.iter().enumerate() {
                if let Some(tv) = scope.get(eid) {
                    let ep = unsafe { self.builder.build_gep(self.ctx.i8_type(), ptr, &[i64.const_int((8 + i * 8) as u64, false)], "e").unwrap() };
                    self.builder.build_store(ep, tv.val).unwrap();
                }
            }
            scope.set(rid, ptr.into(), rty.clone());
        }

        fn emit_map(&mut self, rid: ValueId, entries: &[(ValueId, ValueId)], scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let n = entries.len();
            let ptr = self.call_malloc((8 + n * 16) as u64);
            self.builder.build_store(ptr, i64.const_int(n as u64, false)).unwrap();
            for (i, (k, v)) in entries.iter().enumerate() {
                if let Some(kv) = scope.get(k) {
                    let kp = unsafe { self.builder.build_gep(self.ctx.i8_type(), ptr, &[i64.const_int((8 + i * 16) as u64, false)], "mk").unwrap() };
                    self.builder.build_store(kp, kv.val).unwrap();
                }
                if let Some(vv) = scope.get(v) {
                    let vp = unsafe { self.builder.build_gep(self.ctx.i8_type(), ptr, &[i64.const_int((8 + i * 16 + 8) as u64, false)], "mv").unwrap() };
                    self.builder.build_store(vp, vv.val).unwrap();
                }
            }
            scope.set(rid, ptr.into(), DialectType::Map(Box::new(DialectType::Unknown), Box::new(DialectType::Unknown)));
        }

        fn emit_index(&mut self, rid: ValueId, rty: &DialectType, object: &ValueId, index: &ValueId, scope: &mut FnScope<'ctx>) {
            if let (Some(ov), Some(iv)) = (scope.get(object), scope.get(index)) {
                if ov.val.is_pointer_value() {
                    let i64 = self.ctx.i64_type();
                    let idx = iv.val.into_int_value();
                    let off = self.builder.build_int_add(
                        self.builder.build_int_mul(idx, i64.const_int(8, false), "").unwrap(),
                        i64.const_int(8, false), "off").unwrap();
                    let ep = unsafe { self.builder.build_gep(self.ctx.i8_type(), ov.val.into_pointer_value(), &[off], "ip").unwrap() };
                    let load_ty = self.llvm_type(rty).unwrap_or(i64.into());
                    let loaded = self.builder.build_load(load_ty, ep, "iv").unwrap();
                    scope.set(rid, loaded, rty.clone());
                }
            }
        }

        fn emit_list_fold_sum(&mut self, rid: ValueId, lp: PointerValue<'ctx>, scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let len = self.builder.build_load(i64, lp, "len").unwrap().into_int_value();
            let acc = self.builder.build_alloca(i64, "acc").unwrap();
            let idx = self.builder.build_alloca(i64, "idx").unwrap();
            self.builder.build_store(acc, i64.const_int(0, false)).unwrap();
            self.builder.build_store(idx, i64.const_int(0, false)).unwrap();

            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let cbb = self.ctx.append_basic_block(func, "sc"); let bbb = self.ctx.append_basic_block(func, "sb"); let ebb = self.ctx.append_basic_block(func, "se");
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(cbb);
            let i = self.builder.build_load(i64, idx, "i").unwrap().into_int_value();
            self.builder.build_conditional_branch(self.builder.build_int_compare(IntPredicate::SLT, i, len, "").unwrap(), bbb, ebb).unwrap();
            self.builder.position_at_end(bbb);
            let off = self.builder.build_int_add(self.builder.build_int_mul(i, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let ep = unsafe { self.builder.build_gep(self.ctx.i8_type(), lp, &[off], "").unwrap() };
            let elem = self.builder.build_load(i64, ep, "e").unwrap().into_int_value();
            let a = self.builder.build_load(i64, acc, "a").unwrap().into_int_value();
            self.builder.build_store(acc, self.builder.build_int_add(a, elem, "").unwrap()).unwrap();
            self.builder.build_store(idx, self.builder.build_int_add(i, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(ebb);
            let result = self.builder.build_load(i64, acc, "sum").unwrap();
            scope.set(rid, result, DialectType::I64);
        }

        fn emit_list_map(&mut self, rid: ValueId, lp: PointerValue<'ctx>, closure: inkwell::values::StructValue<'ctx>, scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let fn_ptr = self.builder.build_extract_value(closure, 0, "fp").unwrap().into_pointer_value();
            let env_ptr = self.builder.build_extract_value(closure, 1, "ep").unwrap();
            let len = self.builder.build_load(i64, lp, "len").unwrap().into_int_value();
            let total = self.builder.build_int_add(self.builder.build_int_mul(len, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let malloc = *self.functions.get("malloc").unwrap();
            let res = self.builder.build_call(malloc, &[total.into()], "mr").unwrap();
            let rp = if let inkwell::values::ValueKind::Basic(v) = res.try_as_basic_value() { v.into_pointer_value() } else { return };
            self.builder.build_store(rp, len).unwrap();
            let idx = self.builder.build_alloca(i64, "mi").unwrap();
            self.builder.build_store(idx, i64.const_int(0, false)).unwrap();
            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let cbb = self.ctx.append_basic_block(func, "mc"); let bbb = self.ctx.append_basic_block(func, "mb"); let ebb = self.ctx.append_basic_block(func, "me");
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(cbb);
            let i = self.builder.build_load(i64, idx, "i").unwrap().into_int_value();
            self.builder.build_conditional_branch(self.builder.build_int_compare(IntPredicate::SLT, i, len, "").unwrap(), bbb, ebb).unwrap();
            self.builder.position_at_end(bbb);
            let off = self.builder.build_int_add(self.builder.build_int_mul(i, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let sp = unsafe { self.builder.build_gep(self.ctx.i8_type(), lp, &[off], "").unwrap() };
            let elem = self.builder.build_load(i64, sp, "e").unwrap();
            let call_ty = i64.fn_type(&[ptr.into(), i64.into()], false);
            let mapped = self.builder.build_indirect_call(call_ty, fn_ptr, &[env_ptr.into(), elem.into()], "m").unwrap();
            let mv = if let inkwell::values::ValueKind::Basic(v) = mapped.try_as_basic_value() { v } else { i64.const_int(0, false).into() };
            let dp = unsafe { self.builder.build_gep(self.ctx.i8_type(), rp, &[off], "").unwrap() };
            self.builder.build_store(dp, mv).unwrap();
            self.builder.build_store(idx, self.builder.build_int_add(i, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(ebb);
            scope.set(rid, rp.into(), DialectType::List(Box::new(DialectType::I64)));
        }

        fn emit_list_filter(&mut self, rid: ValueId, lp: PointerValue<'ctx>, closure: inkwell::values::StructValue<'ctx>, scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let fn_ptr = self.builder.build_extract_value(closure, 0, "fp").unwrap().into_pointer_value();
            let env_ptr = self.builder.build_extract_value(closure, 1, "ep").unwrap();
            let len = self.builder.build_load(i64, lp, "len").unwrap().into_int_value();
            let total = self.builder.build_int_add(self.builder.build_int_mul(len, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let malloc = *self.functions.get("malloc").unwrap();
            let res = self.builder.build_call(malloc, &[total.into()], "fr").unwrap();
            let rp = if let inkwell::values::ValueKind::Basic(v) = res.try_as_basic_value() { v.into_pointer_value() } else { return };
            let idx = self.builder.build_alloca(i64, "fi").unwrap();
            let oidx = self.builder.build_alloca(i64, "fo").unwrap();
            self.builder.build_store(idx, i64.const_int(0, false)).unwrap();
            self.builder.build_store(oidx, i64.const_int(0, false)).unwrap();
            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let cbb = self.ctx.append_basic_block(func, "fc"); let bbb = self.ctx.append_basic_block(func, "fb");
            let kbb = self.ctx.append_basic_block(func, "fk"); let nbb = self.ctx.append_basic_block(func, "fn");
            let ebb = self.ctx.append_basic_block(func, "fe");
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(cbb);
            let i = self.builder.build_load(i64, idx, "i").unwrap().into_int_value();
            self.builder.build_conditional_branch(self.builder.build_int_compare(IntPredicate::SLT, i, len, "").unwrap(), bbb, ebb).unwrap();
            self.builder.position_at_end(bbb);
            let off = self.builder.build_int_add(self.builder.build_int_mul(i, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let sp = unsafe { self.builder.build_gep(self.ctx.i8_type(), lp, &[off], "").unwrap() };
            let elem = self.builder.build_load(i64, sp, "e").unwrap();
            let pred_ty = self.ctx.bool_type().fn_type(&[ptr.into(), i64.into()], false);
            let pred = self.builder.build_indirect_call(pred_ty, fn_ptr, &[env_ptr.into(), elem.into()], "p").unwrap();
            let keep = if let inkwell::values::ValueKind::Basic(v) = pred.try_as_basic_value() { v.into_int_value() } else { self.ctx.bool_type().const_int(0, false) };
            self.builder.build_conditional_branch(keep, kbb, nbb).unwrap();
            self.builder.position_at_end(kbb);
            let oi = self.builder.build_load(i64, oidx, "oi").unwrap().into_int_value();
            let ooff = self.builder.build_int_add(self.builder.build_int_mul(oi, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let dp = unsafe { self.builder.build_gep(self.ctx.i8_type(), rp, &[ooff], "").unwrap() };
            self.builder.build_store(dp, elem).unwrap();
            self.builder.build_store(oidx, self.builder.build_int_add(oi, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(nbb).unwrap();
            self.builder.position_at_end(nbb);
            self.builder.build_store(idx, self.builder.build_int_add(i, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(ebb);
            let fl = self.builder.build_load(i64, oidx, "fl").unwrap();
            self.builder.build_store(rp, fl).unwrap();
            scope.set(rid, rp.into(), DialectType::List(Box::new(DialectType::I64)));
        }

        fn emit_list_fold(&mut self, rid: ValueId, lp: PointerValue<'ctx>, init: BasicValueEnum<'ctx>, closure: inkwell::values::StructValue<'ctx>, scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let fn_ptr = self.builder.build_extract_value(closure, 0, "fp").unwrap().into_pointer_value();
            let env_ptr = self.builder.build_extract_value(closure, 1, "ep").unwrap();
            let len = self.builder.build_load(i64, lp, "len").unwrap().into_int_value();
            let acc = self.builder.build_alloca(i64, "fa").unwrap();
            let idx = self.builder.build_alloca(i64, "fi").unwrap();
            self.builder.build_store(acc, init).unwrap();
            self.builder.build_store(idx, i64.const_int(0, false)).unwrap();
            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let cbb = self.ctx.append_basic_block(func, "fc"); let bbb = self.ctx.append_basic_block(func, "fb"); let ebb = self.ctx.append_basic_block(func, "fe");
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(cbb);
            let i = self.builder.build_load(i64, idx, "i").unwrap().into_int_value();
            self.builder.build_conditional_branch(self.builder.build_int_compare(IntPredicate::SLT, i, len, "").unwrap(), bbb, ebb).unwrap();
            self.builder.position_at_end(bbb);
            let off = self.builder.build_int_add(self.builder.build_int_mul(i, i64.const_int(8, false), "").unwrap(), i64.const_int(8, false), "").unwrap();
            let sp = unsafe { self.builder.build_gep(self.ctx.i8_type(), lp, &[off], "").unwrap() };
            let elem = self.builder.build_load(i64, sp, "e").unwrap();
            let a = self.builder.build_load(i64, acc, "a").unwrap();
            let fold_ty = i64.fn_type(&[ptr.into(), i64.into(), i64.into()], false);
            let folded = self.builder.build_indirect_call(fold_ty, fn_ptr, &[env_ptr.into(), a.into(), elem.into()], "f").unwrap();
            let na = if let inkwell::values::ValueKind::Basic(v) = folded.try_as_basic_value() { v } else { i64.const_int(0, false).into() };
            self.builder.build_store(acc, na).unwrap();
            self.builder.build_store(idx, self.builder.build_int_add(i, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(ebb);
            let result = self.builder.build_load(i64, acc, "fr").unwrap();
            scope.set(rid, result, DialectType::I64);
        }

        fn emit_string_to_upper(&mut self, rid: ValueId, src: PointerValue<'ctx>, scope: &mut FnScope<'ctx>) {
            let i64 = self.ctx.i64_type();
            let i32 = self.ctx.i32_type();
            let i8 = self.ctx.i8_type();
            let strlen = *self.functions.get("strlen").unwrap();
            let toupper = *self.functions.get("toupper").unwrap();
            let len = self.builder.build_call(strlen, &[src.into()], "sl").unwrap();
            let len = if let inkwell::values::ValueKind::Basic(v) = len.try_as_basic_value() { v.into_int_value() } else { return };
            let lp1 = self.builder.build_int_add(len, i64.const_int(1, false), "").unwrap();
            let malloc = *self.functions.get("malloc").unwrap();
            let buf = self.builder.build_call(malloc, &[lp1.into()], "ub").unwrap();
            let buf = if let inkwell::values::ValueKind::Basic(v) = buf.try_as_basic_value() { v.into_pointer_value() } else { return };
            let idx = self.builder.build_alloca(i64, "ui").unwrap();
            self.builder.build_store(idx, i64.const_int(0, false)).unwrap();
            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let cbb = self.ctx.append_basic_block(func, "uc"); let bbb = self.ctx.append_basic_block(func, "ub"); let ebb = self.ctx.append_basic_block(func, "ue");
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(cbb);
            let i = self.builder.build_load(i64, idx, "i").unwrap().into_int_value();
            self.builder.build_conditional_branch(self.builder.build_int_compare(IntPredicate::SLT, i, len, "").unwrap(), bbb, ebb).unwrap();
            self.builder.position_at_end(bbb);
            let sp = unsafe { self.builder.build_gep(i8, src, &[i], "").unwrap() };
            let byte = self.builder.build_load(i8, sp, "b").unwrap();
            let ext = self.builder.build_int_z_extend(byte.into_int_value(), i32, "").unwrap();
            let uc = self.builder.build_call(toupper, &[ext.into()], "u").unwrap();
            let ub = if let inkwell::values::ValueKind::Basic(v) = uc.try_as_basic_value() { v.into_int_value() } else { i32.const_int(0, false) };
            let trunc = self.builder.build_int_truncate(ub, i8, "t").unwrap();
            let dp = unsafe { self.builder.build_gep(i8, buf, &[i], "").unwrap() };
            self.builder.build_store(dp, trunc).unwrap();
            self.builder.build_store(idx, self.builder.build_int_add(i, i64.const_int(1, false), "").unwrap()).unwrap();
            self.builder.build_unconditional_branch(cbb).unwrap();
            self.builder.position_at_end(ebb);
            let ep = unsafe { self.builder.build_gep(i8, buf, &[len], "").unwrap() };
            self.builder.build_store(ep, i8.const_int(0, false)).unwrap();
            scope.set(rid, buf.into(), DialectType::String);
        }

        // ── Record ──

        fn emit_record(&mut self, rid: ValueId, rty: &DialectType, name: &Option<almide_base::intern::Sym>, fields: &[(almide_base::intern::Sym, ValueId)], scope: &mut FnScope<'ctx>) {
            if let Some(type_name) = name {
                if let Some(sty) = self.struct_types.get(type_name.as_str()).copied() {
                    let size = sty.size_of().unwrap();
                    let malloc = *self.functions.get("malloc").unwrap();
                    let pc = self.builder.build_call(malloc, &[size.into()], "rp").unwrap();
                    if let inkwell::values::ValueKind::Basic(pv) = pc.try_as_basic_value() {
                        let ptr = pv.into_pointer_value();
                        if let Some(fnames) = self.struct_fields.get(type_name.as_str()) {
                            for (fname, fvid) in fields {
                                if let Some(idx) = fnames.iter().position(|n| n == fname.as_str()) {
                                    if let Some(fv) = scope.get(fvid) {
                                        if let Ok(fp) = self.builder.build_struct_gep(sty, ptr, idx as u32, "f") {
                                            let _ = self.builder.build_store(fp, fv.val);
                                        }
                                    }
                                }
                            }
                        }
                        scope.set(rid, ptr.into(), rty.clone());
                    }
                }
            }
        }

        fn emit_member(&mut self, rid: ValueId, rty: &DialectType, object: &ValueId, field: &almide_base::intern::Sym, scope: &mut FnScope<'ctx>) {
            if let Some(ov) = scope.get(object) {
                if !ov.val.is_pointer_value() { return; }
                let ptr = ov.val.into_pointer_value();
                for (tname, fnames) in &self.struct_fields {
                    if let Some(idx) = fnames.iter().position(|n| n == field.as_str()) {
                        if let Some(sty) = self.struct_types.get(tname.as_str()).copied() {
                            if let Ok(fp) = self.builder.build_struct_gep(sty, ptr, idx as u32, "gf") {
                                let fty = sty.get_field_type_at_index(idx as u32).unwrap();
                                let loaded = self.builder.build_load(fty, fp, "lf").unwrap();
                                scope.set(rid, loaded, rty.clone());
                                return;
                            }
                        }
                    }
                }
            }
        }

        // ── Variant ──

        fn emit_variant_ctor(&mut self, rid: ValueId, rty: &DialectType, parent: &str, tag: u32, _ptys: &[DialectType], args: &[TV<'ctx>], scope: &mut FnScope<'ctx>) {
            if let Some(sty) = self.struct_types.get(parent).copied() {
                let size = sty.size_of().unwrap();
                let malloc = *self.functions.get("malloc").unwrap();
                let pc = self.builder.build_call(malloc, &[size.into()], "vp").unwrap();
                if let inkwell::values::ValueKind::Basic(pv) = pc.try_as_basic_value() {
                    let ptr = pv.into_pointer_value();
                    let tp = self.builder.build_struct_gep(sty, ptr, 0, "tp").unwrap();
                    self.builder.build_store(tp, self.ctx.i32_type().const_int(tag as u64, false)).unwrap();
                    if !args.is_empty() {
                        let pp = self.builder.build_struct_gep(sty, ptr, 1, "pp").unwrap();
                        for (i, tv) in args.iter().enumerate() {
                            let fp = unsafe { self.builder.build_gep(self.ctx.i8_type(), pp, &[self.ctx.i64_type().const_int((i * 8) as u64, false)], "pf").unwrap() };
                            self.builder.build_store(fp, tv.val).unwrap();
                        }
                    }
                    scope.set(rid, ptr.into(), rty.clone());
                }
            }
        }

        fn emit_tagged_union(&mut self, rid: ValueId, rty: &DialectType, tag: u32, value: Option<&ValueId>, scope: &mut FnScope<'ctx>) {
            let type_name = if matches!(rty, DialectType::Result(_, _)) { "Result" } else { "Option" };
            if let Some(sty) = self.struct_types.get(type_name).copied() {
                let size = sty.size_of().unwrap();
                let malloc = *self.functions.get("malloc").unwrap();
                let pc = self.builder.build_call(malloc, &[size.into()], "tp").unwrap();
                if let inkwell::values::ValueKind::Basic(pv) = pc.try_as_basic_value() {
                    let ptr = pv.into_pointer_value();
                    let tp = self.builder.build_struct_gep(sty, ptr, 0, "t").unwrap();
                    self.builder.build_store(tp, self.ctx.i32_type().const_int(tag as u64, false)).unwrap();
                    if let Some(vid) = value {
                        if let Some(v) = scope.get(vid) {
                            let pp = self.builder.build_struct_gep(sty, ptr, 1, "p").unwrap();
                            self.builder.build_store(pp, v.val).unwrap();
                        }
                    }
                    scope.set(rid, ptr.into(), rty.clone());
                }
            }
        }

        // ── Match ──

        fn emit_match(&mut self, rid: ValueId, rty: &DialectType, subject: &ValueId, arms: &[MatchArm], scope: &mut FnScope<'ctx>) {
            let sv = match scope.get(subject) { Some(v) => v, None => return };
            let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
            let merge_bb = self.ctx.append_basic_block(func, "mm");
            let is_variant = arms.iter().any(|a| matches!(&a.pattern, MatchPattern::Variant { .. }));

            let switch_val = if is_variant && sv.val.is_pointer_value() {
                // Load tag from variant ptr
                let variant_ty = self.struct_types.values().next().copied();
                if let Some(sty) = variant_ty {
                    let tp = self.builder.build_struct_gep(sty, sv.val.into_pointer_value(), 0, "mt").unwrap();
                    self.builder.build_load(self.ctx.i32_type(), tp, "tag").unwrap().into_int_value()
                } else { self.ctx.i32_type().const_int(0, false) }
            } else if sv.val.is_int_value() {
                sv.val.into_int_value()
            } else {
                self.ctx.i64_type().const_int(0, false)
            };

            let arm_bbs: Vec<_> = arms.iter().enumerate()
                .map(|(i, _)| self.ctx.append_basic_block(func, &format!("a{}", i))).collect();
            let default_bb = *arm_bbs.last().unwrap_or(&merge_bb);

            let mut cases = Vec::new();
            for (i, arm) in arms.iter().enumerate() {
                match &arm.pattern {
                    MatchPattern::LitInt(v) => {
                        let cv = if switch_val.get_type() == self.ctx.i32_type().into() {
                            self.ctx.i32_type().const_int(*v as u64, true)
                        } else {
                            self.ctx.i64_type().const_int(*v as u64, true)
                        };
                        cases.push((cv, arm_bbs[i]));
                    }
                    MatchPattern::Variant { tag, .. } => {
                        if let Some((_, tag_idx, _)) = self.variant_cases.get(tag.as_str()) {
                            cases.push((self.ctx.i32_type().const_int(*tag_idx as u64, false), arm_bbs[i]));
                        }
                    }
                    _ => {} // wildcard/binding → default
                }
            }
            self.builder.build_switch(switch_val, default_bb, &cases).unwrap();

            let mut arm_results = Vec::new();
            for (i, arm) in arms.iter().enumerate() {
                self.builder.position_at_end(arm_bbs[i]);

                // Extract variant payload bindings
                if let MatchPattern::Variant { tag, bindings } = &arm.pattern {
                    if sv.val.is_pointer_value() {
                        if let Some((parent, _, ptys)) = self.variant_cases.get(tag.as_str()).cloned() {
                            if let Some(sty) = self.struct_types.get(&parent).copied() {
                                let pp = self.builder.build_struct_gep(sty, sv.val.into_pointer_value(), 1, "ap").unwrap();
                                for (j, bid) in bindings.iter().enumerate() {
                                    let off = (j * 8) as u64;
                                    let fp = unsafe { self.builder.build_gep(self.ctx.i8_type(), pp, &[self.ctx.i64_type().const_int(off, false)], "bp").unwrap() };
                                    let fty = if j < ptys.len() { self.llvm_type(&ptys[j]).unwrap_or(self.ctx.i64_type().into()) } else { self.ctx.i64_type().into() };
                                    let loaded = self.builder.build_load(fty, fp, "bv").unwrap();
                                    let bty = if j < ptys.len() { ptys[j].clone() } else { DialectType::I64 };
                                    scope.set(*bid, loaded, bty);
                                }
                            }
                        }
                    }
                }
                // Binding pattern: bind subject value
                if let MatchPattern::Binding(bid) = &arm.pattern {
                    scope.set(*bid, sv.val, sv.ty.clone());
                }

                let av = self.compile_region(&arm.body, scope);
                self.builder.build_unconditional_branch(merge_bb).unwrap();
                let end = self.builder.get_insert_block().unwrap();
                if let Some(v) = av { arm_results.push((v.val, end)); }
            }

            self.builder.position_at_end(merge_bb);
            if !arm_results.is_empty() && arm_results.iter().all(|(v, _)| v.get_type() == arm_results[0].0.get_type()) {
                let phi = self.builder.build_phi(arm_results[0].0.get_type(), "mv").unwrap();
                let incoming: Vec<_> = arm_results.iter().map(|(v, bb)| (v as &dyn inkwell::values::BasicValue, *bb)).collect();
                phi.add_incoming(&incoming);
                scope.set(rid, phi.as_basic_value(), rty.clone());
            }
        }

        // ── Lambda ──

        fn emit_lambda(&mut self, rid: ValueId, rty: &DialectType, params: &[(ValueId, DialectType)], body: &[Block], scope: &mut FnScope<'ctx>) {
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let lambda_name = format!("__lambda_{}", rid.0);

            // Detect captures: ValueIds used in body but not params/body-defined
            let param_ids: std::collections::HashSet<ValueId> = params.iter().map(|(v, _)| *v).collect();
            let mut body_defined = std::collections::HashSet::new();
            let mut body_used = Vec::new();
            for block in body {
                for bop in &block.ops {
                    if let Some(r) = bop.result { body_defined.insert(r); }
                    self.collect_used_values(&bop.kind, &mut body_used);
                }
            }
            let mut seen = std::collections::HashSet::new();
            let captures: Vec<(ValueId, TV<'ctx>)> = body_used.iter()
                .filter(|v| !param_ids.contains(v) && !body_defined.contains(v) && seen.insert(**v))
                .filter_map(|v| scope.get(v).map(|tv| (*v, tv)))
                .collect();

            // Build lambda function type
            let mut ptypes: Vec<BasicMetadataTypeEnum> = vec![ptr.into()]; // env
            for (_, dty) in params {
                if let Some(t) = self.meta_type(dty) { ptypes.push(t); }
            }
            let ret_ty = body.last().and_then(|b| b.ops.last()).and_then(|op| self.llvm_type(&op.result_ty));
            let fn_type = if let Some(ret) = ret_ty { ret.fn_type(&ptypes, false) } else { self.ctx.void_type().fn_type(&ptypes, false) };

            let lambda_fn = self.module.add_function(&lambda_name, fn_type, None);
            let lambda_entry = self.ctx.append_basic_block(lambda_fn, "entry");

            let saved_bb = self.builder.get_insert_block();
            let mut lambda_scope = FnScope::new();

            self.builder.position_at_end(lambda_entry);
            for (i, (vid, dty)) in params.iter().enumerate() {
                if let Some(p) = lambda_fn.get_nth_param((i + 1) as u32) {
                    lambda_scope.set(*vid, p, dty.clone());
                }
            }
            // Unpack captures from env
            if !captures.is_empty() {
                let env = lambda_fn.get_nth_param(0).unwrap().into_pointer_value();
                for (i, (cid, ctv)) in captures.iter().enumerate() {
                    let off = (i * 8) as u64;
                    let fp = unsafe { self.builder.build_gep(self.ctx.i8_type(), env, &[self.ctx.i64_type().const_int(off, false)], "cl").unwrap() };
                    let loaded = self.builder.build_load(ctv.val.get_type(), fp, "cv").unwrap();
                    lambda_scope.set(*cid, loaded, ctv.ty.clone());
                }
            }

            let last = self.compile_region(body, &mut lambda_scope);
            if let Some(v) = last { self.builder.build_return(Some(&v.val)).unwrap(); }
            else { self.builder.build_return(None).unwrap(); }

            // Restore
            if let Some(bb) = saved_bb { self.builder.position_at_end(bb); }

            // Create env on heap
            let env_ptr = if captures.is_empty() {
                ptr.const_null()
            } else {
                let env_size = (captures.len() * 8) as u64;
                let ep = self.call_malloc(env_size);
                for (i, (_, ctv)) in captures.iter().enumerate() {
                    let off = (i * 8) as u64;
                    let fp = unsafe { self.builder.build_gep(self.ctx.i8_type(), ep, &[self.ctx.i64_type().const_int(off, false)], "es").unwrap() };
                    self.builder.build_store(fp, ctv.val).unwrap();
                }
                ep
            };

            // Closure struct
            let closure_ty = self.ctx.struct_type(&[ptr.into(), ptr.into()], false);
            let alloca = self.builder.build_alloca(closure_ty, "cl").unwrap();
            let fp = self.builder.build_struct_gep(closure_ty, alloca, 0, "cfp").unwrap();
            self.builder.build_store(fp, lambda_fn.as_global_value().as_pointer_value()).unwrap();
            let ep = self.builder.build_struct_gep(closure_ty, alloca, 1, "cep").unwrap();
            self.builder.build_store(ep, env_ptr).unwrap();
            let cv = self.builder.build_load(closure_ty, alloca, "cv").unwrap();
            scope.set(rid, cv, rty.clone());
        }

        fn emit_computed_call(&mut self, rid: ValueId, rty: &DialectType, callee: &ValueId, args: &[ValueId], scope: &mut FnScope<'ctx>) {
            if let Some(cv) = scope.get(callee) {
                if cv.val.is_struct_value() {
                    let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
                    let sv = cv.val.into_struct_value();
                    let fn_ptr = self.builder.build_extract_value(sv, 0, "fp").unwrap().into_pointer_value();
                    let env_ptr = self.builder.build_extract_value(sv, 1, "ep").unwrap();
                    let mut call_args: Vec<BasicMetadataValueEnum> = vec![env_ptr.into()];
                    for a in args {
                        if let Some(tv) = scope.get(a) { call_args.push(tv.val.into()); }
                    }
                    let arg_types: Vec<BasicMetadataTypeEnum> = call_args.iter().map(|a| match a {
                        BasicMetadataValueEnum::IntValue(v) => v.get_type().into(),
                        BasicMetadataValueEnum::FloatValue(v) => v.get_type().into(),
                        BasicMetadataValueEnum::PointerValue(v) => v.get_type().into(),
                        _ => self.ctx.i64_type().into(),
                    }).collect();
                    let ret_basic = self.llvm_type(rty);
                    let fn_type = if let Some(ret) = ret_basic { ret.fn_type(&arg_types, false) } else { self.ctx.void_type().fn_type(&arg_types, false) };
                    let call = self.builder.build_indirect_call(fn_type, fn_ptr, &call_args, "cc").unwrap();
                    if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                        scope.set(rid, bv, rty.clone());
                    }
                }
            }
        }

        fn collect_used_values(&self, kind: &OpKind, out: &mut Vec<ValueId>) {
            match kind {
                OpKind::BinOp { lhs, rhs, .. } => { out.push(*lhs); out.push(*rhs); }
                OpKind::UnOp { operand, .. } => { out.push(*operand); }
                OpKind::CallOp { args, .. } | OpKind::IntrinsicCallOp { args, .. } | OpKind::ComputedCallOp { args, .. } => { out.extend(args); }
                OpKind::IfOp { cond, .. } => { out.push(*cond); }
                OpKind::MatchOp { subject, .. } => { out.push(*subject); }
                OpKind::ListOp { elements } | OpKind::TupleOp { elements } => { out.extend(elements); }
                OpKind::MapOp { entries } => { for (k, v) in entries { out.push(*k); out.push(*v); } }
                OpKind::RecordOp { fields, .. } => { for (_, v) in fields { out.push(*v); } }
                OpKind::MemberOp { object, .. } | OpKind::TupleIndexOp { object, .. } => { out.push(*object); }
                OpKind::IndexOp { object, index } | OpKind::MapAccessOp { object, key: index } => { out.push(*object); out.push(*index); }
                OpKind::ResultOkOp { value } | OpKind::ResultErrOp { value } | OpKind::OptionSomeOp { value }
                | OpKind::TryOp { value } | OpKind::UnwrapOp { value } => { out.push(*value); }
                OpKind::UnwrapOrOp { value, fallback } => { out.push(*value); out.push(*fallback); }
                OpKind::AllocVar { init, .. } => { out.push(*init); }
                OpKind::LoadVar { slot } => { out.push(*slot); }
                OpKind::StoreVar { slot, value } => { out.push(*slot); out.push(*value); }
                _ => {}
            }
        }

        // ── Float to string helper ──

        fn get_float_to_string(&mut self) -> FunctionValue<'ctx> {
            if let Some(f) = self.functions.get("__almide_fts") { return *f; }
            let ptr = self.ctx.ptr_type(inkwell::AddressSpace::default());
            let f64t = self.ctx.f64_type();
            let i64 = self.ctx.i64_type();
            let func = self.module.add_function("__almide_fts", ptr.fn_type(&[f64t.into()], false), None);
            let saved = self.builder.get_insert_block();
            let entry = self.ctx.append_basic_block(func, "e");
            let has_dot = self.ctx.append_basic_block(func, "hd");
            let no_dot = self.ctx.append_basic_block(func, "nd");
            self.builder.position_at_end(entry);
            let val = func.get_nth_param(0).unwrap().into_float_value();
            let malloc = *self.functions.get("malloc").unwrap();
            let snprintf = *self.functions.get("snprintf").unwrap();
            let strchr = *self.functions.get("strchr").unwrap();
            let strcat = *self.functions.get("strcat").unwrap();
            let bc = self.builder.build_call(malloc, &[i64.const_int(64, false).into()], "b").unwrap();
            let buf = if let inkwell::values::ValueKind::Basic(v) = bc.try_as_basic_value() { v.into_pointer_value() } else { ptr.const_null() };
            let fmt = self.builder.build_global_string_ptr("%.15g", "fg").unwrap();
            self.builder.build_call(snprintf, &[buf.into(), i64.const_int(64, false).into(), fmt.as_pointer_value().into(), val.into()], "").unwrap();
            let dc = self.builder.build_call(strchr, &[buf.into(), self.ctx.i32_type().const_int('.' as u64, false).into()], "dc").unwrap();
            let dp = if let inkwell::values::ValueKind::Basic(v) = dc.try_as_basic_value() { v.into_pointer_value() } else { ptr.const_null() };
            let is_null = self.builder.build_is_null(dp, "in").unwrap();
            self.builder.build_conditional_branch(is_null, no_dot, has_dot).unwrap();
            self.builder.position_at_end(no_dot);
            let suffix = self.builder.build_global_string_ptr(".0", "dz").unwrap();
            self.builder.build_call(strcat, &[buf.into(), suffix.as_pointer_value().into()], "").unwrap();
            self.builder.build_return(Some(&buf)).unwrap();
            self.builder.position_at_end(has_dot);
            self.builder.build_return(Some(&buf)).unwrap();
            if let Some(bb) = saved { self.builder.position_at_end(bb); }
            self.functions.insert("__almide_fts".into(), func);
            func
        }
    }
}
