//! Dialect Module → LLVM IR via Inkwell.
//!
//! This is the third backend: dialect → native binary, bypassing Rust entirely.
//! Starts with a minimal subset (i64 arithmetic, function calls, control flow)
//! and grows incrementally.

#[cfg(feature = "llvm")]
pub mod codegen {
    use inkwell::context::Context;
    use inkwell::module::Module as LLVMModule;
    use inkwell::builder::Builder;
    use inkwell::values::{BasicValueEnum, FunctionValue, BasicMetadataValueEnum};
    use inkwell::types::{BasicMetadataTypeEnum, BasicType};
    use inkwell::IntPredicate;

    use crate::{Module, ValueId};
    use crate::ops::*;
    use crate::types::DialectType;

    /// Compile a dialect Module to LLVM IR and return the IR as a string.
    pub fn emit_llvm_ir(module: &Module) -> String {
        let context = Context::create();
        let ir = compile_to_module(&context, module);
        ir
    }

    /// Compile a dialect Module to a native object file.
    pub fn emit_object(module: &Module, output_path: &str) -> Result<(), String> {
        use inkwell::targets::*;

        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("Failed to initialize native target: {}", e))?;

        let context = Context::create();
        let llvm_module = context.create_module("almide");
        build_functions(&context, &llvm_module, module);

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("Failed to get target: {}", e))?;
        let machine = target.create_target_machine(
            &triple,
            "generic",
            "",
            inkwell::OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        ).ok_or_else(|| "Failed to create target machine".to_string())?;

        machine.write_to_file(&llvm_module, FileType::Object, std::path::Path::new(output_path))
            .map_err(|e| format!("Failed to write object file: {}", e))
    }

    fn compile_to_module(context: &Context, module: &Module) -> String {
        let llvm_module = context.create_module("almide");
        build_functions(context, &llvm_module, module);
        llvm_module.print_to_string().to_string()
    }

    fn build_functions<'a, 'ctx: 'a>(context: &'ctx Context, llvm_module: &'a LLVMModule<'ctx>, module: &Module) {
        let builder = context.create_builder();
        let mut compiler = LLVMCompiler {
            context,
            module: llvm_module,
            builder: &builder,
            values: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
        };

        compiler.declare_printf();
        for f in &module.functions {
            if f.name.as_str().contains('.') { continue; }
            compiler.declare_function(f);
        }
        for f in &module.functions {
            if f.name.as_str().contains('.') { continue; }
            compiler.compile_function(f);
        }

        // No special entry point — main is emitted with correct C ABI signature.
    }

    struct LLVMCompiler<'a, 'ctx> {
        context: &'ctx Context,
        module: &'a LLVMModule<'ctx>,
        builder: &'a Builder<'ctx>,
        values: std::collections::HashMap<ValueId, BasicValueEnum<'ctx>>,
        functions: std::collections::HashMap<String, FunctionValue<'ctx>>,
    }

    impl<'a, 'ctx: 'a> LLVMCompiler<'a, 'ctx> {
        fn declare_printf(&mut self) {
            let i32_type = self.context.i32_type();
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
            let printf_type = i32_type.fn_type(&[ptr_type.into()], true);
            let printf = self.module.add_function("printf", printf_type, None);
            self.functions.insert("printf".into(), printf);
        }

        fn dialect_to_llvm_type(&self, ty: &DialectType) -> Option<BasicMetadataTypeEnum<'ctx>> {
            match ty {
                DialectType::I64 => Some(self.context.i64_type().into()),
                DialectType::F64 => Some(self.context.f64_type().into()),
                DialectType::Bool => Some(self.context.bool_type().into()),
                DialectType::I32 => Some(self.context.i32_type().into()),
                DialectType::I8 => Some(self.context.i8_type().into()),
                _ => None, // String, List, etc. need heap — future work
            }
        }

        fn dialect_to_basic_type(&self, ty: &DialectType) -> Option<inkwell::types::BasicTypeEnum<'ctx>> {
            match ty {
                DialectType::I64 => Some(self.context.i64_type().into()),
                DialectType::F64 => Some(self.context.f64_type().into()),
                DialectType::Bool => Some(self.context.bool_type().into()),
                DialectType::I32 => Some(self.context.i32_type().into()),
                DialectType::I8 => Some(self.context.i8_type().into()),
                _ => None,
            }
        }

        fn declare_function(&mut self, f: &crate::ops::FuncOp) {
            let params: Vec<BasicMetadataTypeEnum<'ctx>> = f.params.iter()
                .filter_map(|(_, ty)| self.dialect_to_llvm_type(ty))
                .collect();

            let fn_type = if f.name.as_str() == "main" {
                // C ABI: main returns i32
                self.context.i32_type().fn_type(&params, false)
            } else if matches!(f.ret_ty, DialectType::Unit) {
                self.context.void_type().fn_type(&params, false)
            } else if let Some(ret) = self.dialect_to_basic_type(&f.ret_ty) {
                ret.fn_type(&params, false)
            } else {
                self.context.void_type().fn_type(&params, false)
            };

            let function = self.module.add_function(f.name.as_str(), fn_type, None);
            self.functions.insert(f.name.as_str().to_string(), function);
        }

        fn compile_function(&mut self, f: &crate::ops::FuncOp) {
            let function = match self.functions.get(f.name.as_str()) {
                Some(f) => *f,
                None => return,
            };

            let entry = self.context.append_basic_block(function, "entry");
            self.builder.position_at_end(entry);
            self.values.clear();

            // Map params to ValueIds
            if let Some(block) = f.body.first() {
                for (i, (val_id, _)) in block.args.iter().enumerate() {
                    if let Some(param) = function.get_nth_param(i as u32) {
                        self.values.insert(*val_id, param);
                    }
                }
            }

            // Compile body
            for block in &f.body {
                for op in &block.ops {
                    self.compile_op(op);
                }
                // Terminator
                match &block.terminator {
                    Terminator::Return(v) => {
                        if f.name.as_str() == "main" {
                            // C ABI: return 0
                            let zero = self.context.i32_type().const_int(0, false);
                            self.builder.build_return(Some(&zero)).unwrap();
                        } else if matches!(f.ret_ty, DialectType::Unit) {
                            self.builder.build_return(None).unwrap();
                        } else if let Some(val) = self.values.get(v) {
                            self.builder.build_return(Some(val)).unwrap();
                        } else {
                            self.builder.build_return(None).unwrap();
                        }
                    }
                    _ => {
                        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                            if f.name.as_str() == "main" {
                                let zero = self.context.i32_type().const_int(0, false);
                                self.builder.build_return(Some(&zero)).unwrap();
                            } else {
                                self.builder.build_return(None).unwrap();
                            }
                        }
                    }
                }
            }
        }

        fn compile_op(&mut self, op: &Operation) {
            let result_id = match op.result {
                Some(id) => id,
                None => return,
            };

            match &op.kind {
                OpKind::ConstInt(v) => {
                    let val = self.context.i64_type().const_int(*v as u64, true);
                    self.values.insert(result_id, val.into());
                }
                OpKind::ConstFloat(v) => {
                    let val = self.context.f64_type().const_float(*v);
                    self.values.insert(result_id, val.into());
                }
                OpKind::ConstBool(v) => {
                    let val = self.context.bool_type().const_int(*v as u64, false);
                    self.values.insert(result_id, val.into());
                }
                OpKind::ConstUnit => {
                    // Unit is void — no value to store
                }

                OpKind::BinOp { op, lhs, rhs } => {
                    let l = self.values.get(lhs).cloned();
                    let r = self.values.get(rhs).cloned();
                    if let (Some(l), Some(r)) = (l, r) {
                        let result = match op {
                            almide_ir::BinOp::AddInt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_add(lv, rv, "add").unwrap().into())
                            }
                            almide_ir::BinOp::SubInt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_sub(lv, rv, "sub").unwrap().into())
                            }
                            almide_ir::BinOp::MulInt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_mul(lv, rv, "mul").unwrap().into())
                            }
                            almide_ir::BinOp::DivInt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_signed_div(lv, rv, "div").unwrap().into())
                            }
                            almide_ir::BinOp::ModInt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_signed_rem(lv, rv, "rem").unwrap().into())
                            }
                            almide_ir::BinOp::AddFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                Some(self.builder.build_float_add(lv, rv, "fadd").unwrap().into())
                            }
                            almide_ir::BinOp::SubFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                Some(self.builder.build_float_sub(lv, rv, "fsub").unwrap().into())
                            }
                            almide_ir::BinOp::MulFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                Some(self.builder.build_float_mul(lv, rv, "fmul").unwrap().into())
                            }
                            almide_ir::BinOp::DivFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                Some(self.builder.build_float_div(lv, rv, "fdiv").unwrap().into())
                            }
                            almide_ir::BinOp::Eq => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::EQ, lv, rv, "eq").unwrap().into())
                            }
                            almide_ir::BinOp::Lt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::SLT, lv, rv, "lt").unwrap().into())
                            }
                            almide_ir::BinOp::Gt => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::SGT, lv, rv, "gt").unwrap().into())
                            }
                            almide_ir::BinOp::Lte => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::SLE, lv, rv, "lte").unwrap().into())
                            }
                            almide_ir::BinOp::Gte => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::SGE, lv, rv, "gte").unwrap().into())
                            }
                            almide_ir::BinOp::Neq => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_int_compare(IntPredicate::NE, lv, rv, "neq").unwrap().into())
                            }
                            almide_ir::BinOp::And => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_and(lv, rv, "and").unwrap().into())
                            }
                            almide_ir::BinOp::Or => {
                                let lv = l.into_int_value();
                                let rv = r.into_int_value();
                                Some(self.builder.build_or(lv, rv, "or").unwrap().into())
                            }
                            _ => None,
                        };
                        if let Some(val) = result {
                            self.values.insert(result_id, val);
                        }
                    }
                }

                OpKind::CallOp { callee, args } => {
                    let callee_str = callee.as_str();
                    if callee_str == "println" {
                        // println with printf: %lld\n for ints, %f\n for floats
                        if let Some(arg) = args.first().and_then(|a| self.values.get(a)) {
                            let fmt = match arg {
                                BasicValueEnum::IntValue(_) => self.builder.build_global_string_ptr("%lld\n", "fmt_int").unwrap(),
                                BasicValueEnum::FloatValue(_) => self.builder.build_global_string_ptr("%.17g\n", "fmt_float").unwrap(),
                                _ => self.builder.build_global_string_ptr("%s\n", "fmt_str").unwrap(),
                            };
                            if let Some(printf) = self.functions.get("printf") {
                                self.builder.build_call(
                                    *printf,
                                    &[fmt.as_pointer_value().into(), (*arg).into()],
                                    "printf_call",
                                ).unwrap();
                            }
                        }
                    } else if callee_str == "int.to_string" || callee_str == "float.to_string" {
                        // Pass through: the value stays as Int/Float, println handles formatting
                        if let Some(val) = args.first().and_then(|a| self.values.get(a)) {
                            self.values.insert(result_id, *val);
                        }
                    } else if let Some(func) = self.functions.get(callee_str).copied() {
                        let arg_vals: Vec<BasicMetadataValueEnum> = args.iter()
                            .filter_map(|a| self.values.get(a).map(|v| (*v).into()))
                            .collect();
                        let call = self.builder.build_call(func, &arg_vals, "call").unwrap();
                        if func.get_type().get_return_type().is_some() {
                            if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                                self.values.insert(result_id, bv);
                            }
                        }
                    }
                }

                OpKind::IfOp { cond, then_region, else_region } => {
                    if let Some(cond_val) = self.values.get(cond) {
                        let cond_int = cond_val.into_int_value();
                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let then_bb = self.context.append_basic_block(function, "then");
                        let else_bb = self.context.append_basic_block(function, "else");
                        let merge_bb = self.context.append_basic_block(function, "merge");

                        self.builder.build_conditional_branch(cond_int, then_bb, else_bb).unwrap();

                        // Then
                        self.builder.position_at_end(then_bb);
                        let mut then_val = None;
                        for block in then_region {
                            for op in &block.ops { self.compile_op(op); }
                            if let Terminator::Yield(v) = &block.terminator {
                                then_val = self.values.get(v).cloned();
                            }
                        }
                        self.builder.build_unconditional_branch(merge_bb).unwrap();
                        let then_end = self.builder.get_insert_block().unwrap();

                        // Else
                        self.builder.position_at_end(else_bb);
                        let mut else_val = None;
                        for block in else_region {
                            for op in &block.ops { self.compile_op(op); }
                            if let Terminator::Yield(v) = &block.terminator {
                                else_val = self.values.get(v).cloned();
                            }
                        }
                        self.builder.build_unconditional_branch(merge_bb).unwrap();
                        let else_end = self.builder.get_insert_block().unwrap();

                        // Merge with phi
                        self.builder.position_at_end(merge_bb);
                        if let (Some(tv), Some(ev)) = (then_val, else_val) {
                            let phi = self.builder.build_phi(tv.get_type(), "ifval").unwrap();
                            phi.add_incoming(&[(&tv, then_end), (&ev, else_end)]);
                            self.values.insert(result_id, phi.as_basic_value());
                        }
                    }
                }

                _ => {} // TODO: match, lambda, collections, etc.
            }
        }
    }
}
