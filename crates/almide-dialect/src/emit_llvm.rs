//! Dialect Module → LLVM IR via Inkwell.
//!
//! This is the third backend: dialect → native binary, bypassing Rust entirely.
//! Starts with a minimal subset (i64 arithmetic, function calls, control flow)
//! and grows incrementally.

#[cfg(feature = "llvm")]
pub mod codegen {
    use inkwell::values::AsValueRef;
    use inkwell::context::Context;

    /// Set fast-math flags on a float instruction for auto-vectorization.
    fn set_fast_math(val: inkwell::values::FloatValue) {
        unsafe {
            let flags = llvm_sys::LLVMFastMathAllowReassoc | llvm_sys::LLVMFastMathNoNaNs | llvm_sys::LLVMFastMathNoInfs;
            llvm_sys::core::LLVMSetFastMathFlags(val.as_value_ref(), flags);
        }
    }
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

    /// Compile a dialect Module to a native object file with LLVM optimizations.
    pub fn emit_object(module: &Module, output_path: &str) -> Result<(), String> {
        use inkwell::targets::*;
        use inkwell::passes::PassBuilderOptions;

        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("Failed to initialize native target: {}", e))?;

        let context = Context::create();
        let llvm_module = context.create_module("almide");
        build_functions(&context, &llvm_module, module);

        let triple = TargetMachine::get_default_triple();
        let cpu = TargetMachine::get_host_cpu_features();
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("Failed to get target: {}", e))?;
        let machine = target.create_target_machine(
            &triple,
            TargetMachine::get_host_cpu_name().to_str().unwrap_or("generic"),
            cpu.to_str().unwrap_or(""),
            inkwell::OptimizationLevel::Aggressive,
            RelocMode::Default,
            CodeModel::Default,
        ).ok_or_else(|| "Failed to create target machine".to_string())?;

        // Run LLVM optimization passes: O2 pipeline with vectorization
        let opts = PassBuilderOptions::create();
        llvm_module.run_passes("default<O2>", &machine, opts)
            .map_err(|e| format!("LLVM pass error: {}", e))?;

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
            allocas: std::collections::HashMap::new(),
            struct_types: std::collections::HashMap::new(),
            struct_fields: std::collections::HashMap::new(),
            variant_cases: std::collections::HashMap::new(),
        };

        // Register struct types from type declarations
        for td in &module.type_decls {
            if let crate::ops::TypeDeclKind::Record { fields } = &td.kind {
                let field_types: Vec<inkwell::types::BasicTypeEnum> = fields.iter()
                    .filter_map(|(_, ty)| compiler.dialect_to_basic_type(ty))
                    .collect();
                let struct_ty = context.opaque_struct_type(td.name.as_str());
                struct_ty.set_body(&field_types, false);
                compiler.struct_types.insert(td.name.as_str().to_string(), struct_ty);
                compiler.struct_fields.insert(
                    td.name.as_str().to_string(),
                    fields.iter().map(|(n, _)| n.as_str().to_string()).collect(),
                );
            }
        }

        // Register variant types
        for td in &module.type_decls {
            if let crate::ops::TypeDeclKind::Variant { cases } = &td.kind {
                // Compute max payload size (in bytes, assuming 8 bytes per field)
                let max_payload = cases.iter().map(|c| c.payload.len() * 8).max().unwrap_or(0);
                // Variant struct: { i32 tag, [max_payload x i8] }
                let i32_ty = context.i32_type();
                let payload_ty = context.i8_type().array_type(max_payload as u32);
                let variant_ty = context.opaque_struct_type(td.name.as_str());
                variant_ty.set_body(&[i32_ty.into(), payload_ty.into()], false);
                compiler.struct_types.insert(td.name.as_str().to_string(), variant_ty);
                // Store case names → tag indices + payload types
                let case_names: Vec<String> = cases.iter().map(|c| c.name.as_str().to_string()).collect();
                compiler.struct_fields.insert(td.name.as_str().to_string(), case_names);
                for (i, case) in cases.iter().enumerate() {
                    compiler.variant_cases.insert(
                        case.name.as_str().to_string(),
                        (td.name.as_str().to_string(), i as u32, case.payload.clone()),
                    );
                }
            }
        }

        // Register built-in tagged union types (Result, Option)
        {
            let i32_ty = context.i32_type();
            let payload_ty = context.i8_type().array_type(16); // 16 bytes covers i64 or ptr
            let result_ty = context.opaque_struct_type("Result");
            result_ty.set_body(&[i32_ty.into(), payload_ty.into()], false);
            compiler.struct_types.insert("Result".to_string(), result_ty);
            // Ok = tag 0, Err = tag 1
            compiler.variant_cases.insert("Ok".to_string(), ("Result".to_string(), 0, vec![DialectType::I64]));
            compiler.variant_cases.insert("Err".to_string(), ("Result".to_string(), 1, vec![DialectType::String]));

            let option_ty = context.opaque_struct_type("Option");
            option_ty.set_body(&[i32_ty.into(), payload_ty.into()], false);
            compiler.struct_types.insert("Option".to_string(), option_ty);
            compiler.variant_cases.insert("Some".to_string(), ("Option".to_string(), 0, vec![DialectType::I64]));
            compiler.variant_cases.insert("None".to_string(), ("Option".to_string(), 1, vec![]));
        }

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
        allocas: std::collections::HashMap<ValueId, inkwell::values::PointerValue<'ctx>>,
        /// Named type → LLVM struct type mapping.
        struct_types: std::collections::HashMap<String, inkwell::types::StructType<'ctx>>,
        /// Named type → field names (ordered). For records: field names. For variants: case names.
        struct_fields: std::collections::HashMap<String, Vec<String>>,
        /// Variant case name → (parent type name, tag index, payload types)
        variant_cases: std::collections::HashMap<String, (String, u32, Vec<DialectType>)>,
    }

    impl<'a, 'ctx: 'a> LLVMCompiler<'a, 'ctx> {
        /// Emit or retrieve the __almide_float_to_string helper function.
        /// Matches Rust behavior: if no '.', append ".0".
        fn get_or_emit_float_to_string(&mut self) -> FunctionValue<'ctx> {
            if let Some(f) = self.functions.get("__almide_fts") {
                return *f;
            }
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
            let f64_type = self.context.f64_type();
            let fn_type = ptr_type.fn_type(&[f64_type.into()], false);
            let func = self.module.add_function("__almide_fts", fn_type, None);

            let saved_bb = self.builder.get_insert_block();
            let entry = self.context.append_basic_block(func, "entry");
            let has_dot = self.context.append_basic_block(func, "has_dot");
            let no_dot = self.context.append_basic_block(func, "no_dot");

            self.builder.position_at_end(entry);
            let val = func.get_nth_param(0).unwrap().into_float_value();
            let i64_type = self.context.i64_type();

            let malloc = *self.functions.get("malloc").unwrap();
            let snprintf = *self.functions.get("snprintf").unwrap();
            let strcat = *self.functions.get("strcat").unwrap();
            let buf_call = self.builder.build_call(malloc, &[i64_type.const_int(64, false).into()], "buf").unwrap();
            let buf = if let inkwell::values::ValueKind::Basic(v) = buf_call.try_as_basic_value() { v.into_pointer_value() } else { ptr_type.const_null() };
            // %.15g gives precision close to Rust's Display for f64
            let fmt = self.builder.build_global_string_ptr("%.15g", "fts_fmt").unwrap();
            self.builder.build_call(snprintf, &[buf.into(), i64_type.const_int(64, false).into(), fmt.as_pointer_value().into(), val.into()], "").unwrap();

            // Check if buf contains '.' using strchr
            // Declare strchr
            let strchr_ty = ptr_type.fn_type(&[ptr_type.into(), self.context.i32_type().into()], false);
            let strchr = self.module.add_function("strchr", strchr_ty, None);
            let dot_result = self.builder.build_call(strchr, &[buf.into(), self.context.i32_type().const_int('.' as u64, false).into()], "dot_check").unwrap();
            let dot_ptr = if let inkwell::values::ValueKind::Basic(v) = dot_result.try_as_basic_value() { v.into_pointer_value() } else { ptr_type.const_null() };
            let is_null = self.builder.build_is_null(dot_ptr, "is_null").unwrap();
            self.builder.build_conditional_branch(is_null, no_dot, has_dot).unwrap();

            // no_dot: strcat(buf, ".0"), return buf
            self.builder.position_at_end(no_dot);
            let suffix = self.builder.build_global_string_ptr(".0", "dot_zero").unwrap();
            // strcat already copied above
            self.builder.build_call(strcat, &[buf.into(), suffix.as_pointer_value().into()], "").unwrap();
            self.builder.build_return(Some(&buf)).unwrap();

            // has_dot: return buf as-is
            self.builder.position_at_end(has_dot);
            self.builder.build_return(Some(&buf)).unwrap();

            if let Some(bb) = saved_bb {
                self.builder.position_at_end(bb);
            }
            self.functions.insert("__almide_fts".to_string(), func);
            func
        }

        /// Emit inline LLVM for stdlib operations (list.*, string.*).
        fn emit_stdlib_call(&mut self, result_id: ValueId, func: &str, args: &[BasicValueEnum<'ctx>], result_ty: &DialectType) {
            let i64_type = self.context.i64_type();
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());

            match func {
                "list.len" => {
                    // Load length from list ptr (first i64)
                    if let Some(list_ptr) = args.first() {
                        let len = self.builder.build_load(i64_type, list_ptr.into_pointer_value(), "list_len").unwrap();
                        self.values.insert(result_id, len);
                    }
                }
                "list.sum" => {
                    // Sum all i64 elements: loop from 0 to len, accumulate
                    if let Some(list_ptr) = args.first() {
                        let lp = list_ptr.into_pointer_value();
                        let len = self.builder.build_load(i64_type, lp, "len").unwrap().into_int_value();

                        // Alloca for accumulator and index
                        let acc_alloca = self.builder.build_alloca(i64_type, "sum_acc").unwrap();
                        let idx_alloca = self.builder.build_alloca(i64_type, "sum_idx").unwrap();
                        self.builder.build_store(acc_alloca, i64_type.const_int(0, false)).unwrap();
                        self.builder.build_store(idx_alloca, i64_type.const_int(0, false)).unwrap();

                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let cond_bb = self.context.append_basic_block(function, "sum_cond");
                        let body_bb = self.context.append_basic_block(function, "sum_body");
                        let exit_bb = self.context.append_basic_block(function, "sum_exit");

                        self.builder.build_unconditional_branch(cond_bb).unwrap();
                        self.builder.position_at_end(cond_bb);
                        let idx = self.builder.build_load(i64_type, idx_alloca, "i").unwrap().into_int_value();
                        let cmp = self.builder.build_int_compare(IntPredicate::SLT, idx, len, "cmp").unwrap();
                        self.builder.build_conditional_branch(cmp, body_bb, exit_bb).unwrap();

                        self.builder.position_at_end(body_bb);
                        let offset = self.builder.build_int_add(self.builder.build_int_mul(idx, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "off").unwrap();
                        let elem_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), lp, &[offset], "ep").unwrap() };
                        let elem = self.builder.build_load(i64_type, elem_ptr, "elem").unwrap().into_int_value();
                        let acc = self.builder.build_load(i64_type, acc_alloca, "acc").unwrap().into_int_value();
                        let new_acc = self.builder.build_int_add(acc, elem, "new_acc").unwrap();
                        self.builder.build_store(acc_alloca, new_acc).unwrap();
                        let new_idx = self.builder.build_int_add(idx, i64_type.const_int(1, false), "new_i").unwrap();
                        self.builder.build_store(idx_alloca, new_idx).unwrap();
                        self.builder.build_unconditional_branch(cond_bb).unwrap();

                        self.builder.position_at_end(exit_bb);
                        let result = self.builder.build_load(i64_type, acc_alloca, "sum_result").unwrap();
                        self.values.insert(result_id, result);
                    }
                }
                "list.map" => {
                    // map(list, closure) → new list with closure applied to each element
                    if args.len() >= 2 {
                        let lp = args[0].into_pointer_value();
                        let closure = args[1].into_struct_value();
                        let fn_ptr = self.builder.build_extract_value(closure, 0, "map_fn").unwrap().into_pointer_value();
                        let env_ptr = self.builder.build_extract_value(closure, 1, "map_env").unwrap();

                        let len = self.builder.build_load(i64_type, lp, "len").unwrap().into_int_value();
                        let malloc = *self.functions.get("malloc").unwrap();

                        // Allocate result list
                        let total = self.builder.build_int_add(self.builder.build_int_mul(len, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "total").unwrap();
                        let res_call = self.builder.build_call(malloc, &[total.into()], "map_res").unwrap();
                        let res_ptr = if let inkwell::values::ValueKind::Basic(v) = res_call.try_as_basic_value() { v.into_pointer_value() } else { ptr_type.const_null() };
                        self.builder.build_store(res_ptr, len).unwrap();

                        // Loop
                        let idx_alloca = self.builder.build_alloca(i64_type, "map_i").unwrap();
                        self.builder.build_store(idx_alloca, i64_type.const_int(0, false)).unwrap();
                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let cond_bb = self.context.append_basic_block(function, "map_cond");
                        let body_bb = self.context.append_basic_block(function, "map_body");
                        let exit_bb = self.context.append_basic_block(function, "map_exit");

                        self.builder.build_unconditional_branch(cond_bb).unwrap();
                        self.builder.position_at_end(cond_bb);
                        let idx = self.builder.build_load(i64_type, idx_alloca, "i").unwrap().into_int_value();
                        let cmp = self.builder.build_int_compare(IntPredicate::SLT, idx, len, "cmp").unwrap();
                        self.builder.build_conditional_branch(cmp, body_bb, exit_bb).unwrap();

                        self.builder.position_at_end(body_bb);
                        let offset = self.builder.build_int_add(self.builder.build_int_mul(idx, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "off").unwrap();
                        let src_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), lp, &[offset], "sp").unwrap() };
                        let elem = self.builder.build_load(i64_type, src_ptr, "elem").unwrap();

                        // Call closure: fn_ptr(env_ptr, elem)
                        let call_ty = i64_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
                        let mapped = self.builder.build_indirect_call(call_ty, fn_ptr, &[env_ptr.into(), elem.into()], "mapped").unwrap();
                        let mapped_val = if let inkwell::values::ValueKind::Basic(v) = mapped.try_as_basic_value() { v } else { i64_type.const_int(0, false).into() };

                        let dst_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), res_ptr, &[offset], "dp").unwrap() };
                        self.builder.build_store(dst_ptr, mapped_val).unwrap();

                        let new_idx = self.builder.build_int_add(idx, i64_type.const_int(1, false), "").unwrap();
                        self.builder.build_store(idx_alloca, new_idx).unwrap();
                        self.builder.build_unconditional_branch(cond_bb).unwrap();

                        self.builder.position_at_end(exit_bb);
                        self.values.insert(result_id, res_ptr.into());
                    }
                }
                "list.filter" => {
                    // filter(list, closure) → new list with elements where closure returns true
                    if args.len() >= 2 {
                        let lp = args[0].into_pointer_value();
                        let closure = args[1].into_struct_value();
                        let fn_ptr = self.builder.build_extract_value(closure, 0, "filt_fn").unwrap().into_pointer_value();
                        let env_ptr = self.builder.build_extract_value(closure, 1, "filt_env").unwrap();

                        let len = self.builder.build_load(i64_type, lp, "len").unwrap().into_int_value();
                        let malloc = *self.functions.get("malloc").unwrap();

                        // Allocate max-size result list
                        let total = self.builder.build_int_add(self.builder.build_int_mul(len, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "total").unwrap();
                        let res_call = self.builder.build_call(malloc, &[total.into()], "filt_res").unwrap();
                        let res_ptr = if let inkwell::values::ValueKind::Basic(v) = res_call.try_as_basic_value() { v.into_pointer_value() } else { ptr_type.const_null() };

                        let idx_alloca = self.builder.build_alloca(i64_type, "filt_i").unwrap();
                        let out_idx_alloca = self.builder.build_alloca(i64_type, "filt_out").unwrap();
                        self.builder.build_store(idx_alloca, i64_type.const_int(0, false)).unwrap();
                        self.builder.build_store(out_idx_alloca, i64_type.const_int(0, false)).unwrap();

                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let cond_bb = self.context.append_basic_block(function, "filt_cond");
                        let body_bb = self.context.append_basic_block(function, "filt_body");
                        let keep_bb = self.context.append_basic_block(function, "filt_keep");
                        let next_bb = self.context.append_basic_block(function, "filt_next");
                        let exit_bb = self.context.append_basic_block(function, "filt_exit");

                        self.builder.build_unconditional_branch(cond_bb).unwrap();
                        self.builder.position_at_end(cond_bb);
                        let idx = self.builder.build_load(i64_type, idx_alloca, "i").unwrap().into_int_value();
                        let cmp = self.builder.build_int_compare(IntPredicate::SLT, idx, len, "cmp").unwrap();
                        self.builder.build_conditional_branch(cmp, body_bb, exit_bb).unwrap();

                        self.builder.position_at_end(body_bb);
                        let offset = self.builder.build_int_add(self.builder.build_int_mul(idx, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "off").unwrap();
                        let src_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), lp, &[offset], "sp").unwrap() };
                        let elem = self.builder.build_load(i64_type, src_ptr, "elem").unwrap();

                        // Call predicate: fn_ptr(env_ptr, elem) → bool (i1)
                        let bool_type = self.context.bool_type();
                        let pred_ty = bool_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
                        let pred_result = self.builder.build_indirect_call(pred_ty, fn_ptr, &[env_ptr.into(), elem.into()], "pred").unwrap();
                        let keep = if let inkwell::values::ValueKind::Basic(v) = pred_result.try_as_basic_value() { v.into_int_value() } else { bool_type.const_int(0, false) };
                        self.builder.build_conditional_branch(keep, keep_bb, next_bb).unwrap();

                        self.builder.position_at_end(keep_bb);
                        let out_idx = self.builder.build_load(i64_type, out_idx_alloca, "out_i").unwrap().into_int_value();
                        let out_offset = self.builder.build_int_add(self.builder.build_int_mul(out_idx, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "out_off").unwrap();
                        let dst_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), res_ptr, &[out_offset], "dp").unwrap() };
                        self.builder.build_store(dst_ptr, elem).unwrap();
                        let new_out = self.builder.build_int_add(out_idx, i64_type.const_int(1, false), "").unwrap();
                        self.builder.build_store(out_idx_alloca, new_out).unwrap();
                        self.builder.build_unconditional_branch(next_bb).unwrap();

                        self.builder.position_at_end(next_bb);
                        let new_idx = self.builder.build_int_add(idx, i64_type.const_int(1, false), "").unwrap();
                        self.builder.build_store(idx_alloca, new_idx).unwrap();
                        self.builder.build_unconditional_branch(cond_bb).unwrap();

                        self.builder.position_at_end(exit_bb);
                        let final_len = self.builder.build_load(i64_type, out_idx_alloca, "final_len").unwrap();
                        self.builder.build_store(res_ptr, final_len).unwrap();
                        self.values.insert(result_id, res_ptr.into());
                    }
                }
                "list.fold" => {
                    // fold(list, init, closure) → accumulator
                    if args.len() >= 3 {
                        let lp = args[0].into_pointer_value();
                        let init = args[1];
                        let closure = args[2].into_struct_value();
                        let fn_ptr = self.builder.build_extract_value(closure, 0, "fold_fn").unwrap().into_pointer_value();
                        let env_ptr = self.builder.build_extract_value(closure, 1, "fold_env").unwrap();

                        let len = self.builder.build_load(i64_type, lp, "len").unwrap().into_int_value();
                        let acc_alloca = self.builder.build_alloca(i64_type, "fold_acc").unwrap();
                        let idx_alloca = self.builder.build_alloca(i64_type, "fold_i").unwrap();
                        self.builder.build_store(acc_alloca, init).unwrap();
                        self.builder.build_store(idx_alloca, i64_type.const_int(0, false)).unwrap();

                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let cond_bb = self.context.append_basic_block(function, "fold_cond");
                        let body_bb = self.context.append_basic_block(function, "fold_body");
                        let exit_bb = self.context.append_basic_block(function, "fold_exit");

                        self.builder.build_unconditional_branch(cond_bb).unwrap();
                        self.builder.position_at_end(cond_bb);
                        let idx = self.builder.build_load(i64_type, idx_alloca, "i").unwrap().into_int_value();
                        let cmp = self.builder.build_int_compare(IntPredicate::SLT, idx, len, "cmp").unwrap();
                        self.builder.build_conditional_branch(cmp, body_bb, exit_bb).unwrap();

                        self.builder.position_at_end(body_bb);
                        let offset = self.builder.build_int_add(self.builder.build_int_mul(idx, i64_type.const_int(8, false), "").unwrap(), i64_type.const_int(8, false), "off").unwrap();
                        let src_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), lp, &[offset], "sp").unwrap() };
                        let elem = self.builder.build_load(i64_type, src_ptr, "elem").unwrap();
                        let acc = self.builder.build_load(i64_type, acc_alloca, "acc").unwrap();

                        // Call closure: fn_ptr(env_ptr, acc, elem)
                        let fold_ty = i64_type.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false);
                        let folded = self.builder.build_indirect_call(fold_ty, fn_ptr, &[env_ptr.into(), acc.into(), elem.into()], "folded").unwrap();
                        let new_acc = if let inkwell::values::ValueKind::Basic(v) = folded.try_as_basic_value() { v } else { i64_type.const_int(0, false).into() };
                        self.builder.build_store(acc_alloca, new_acc).unwrap();

                        let new_idx = self.builder.build_int_add(idx, i64_type.const_int(1, false), "").unwrap();
                        self.builder.build_store(idx_alloca, new_idx).unwrap();
                        self.builder.build_unconditional_branch(cond_bb).unwrap();

                        self.builder.position_at_end(exit_bb);
                        let result = self.builder.build_load(i64_type, acc_alloca, "fold_result").unwrap();
                        self.values.insert(result_id, result);
                    }
                }
                "math.sqrt" => {
                    if let Some(val) = args.first() {
                        // Declare sqrt from libm
                        let f64_type = self.context.f64_type();
                        let sqrt_ty = f64_type.fn_type(&[f64_type.into()], false);
                        let sqrt_fn = self.module.add_function("sqrt", sqrt_ty, None);
                        let result = self.builder.build_call(sqrt_fn, &[(*val).into()], "sqrt").unwrap();
                        if let inkwell::values::ValueKind::Basic(v) = result.try_as_basic_value() {
                            self.values.insert(result_id, v);
                        }
                    }
                }
                "map.len" => {
                    // Map layout: [i64 len][entries...]
                    // Same as list for the header
                    if let Some(map_ptr) = args.first() {
                        let len = self.builder.build_load(i64_type, map_ptr.into_pointer_value(), "map_len").unwrap();
                        self.values.insert(result_id, len);
                    }
                }
                "string.to_upper" => {
                    // toupper each char: malloc + loop
                    if let Some(str_val) = args.first() {
                        let src = str_val.into_pointer_value();
                        let strlen_fn = *self.functions.get("strlen").unwrap();
                        let malloc = *self.functions.get("malloc").unwrap();
                        let len_call = self.builder.build_call(strlen_fn, &[src.into()], "slen").unwrap();
                        let len = if let inkwell::values::ValueKind::Basic(v) = len_call.try_as_basic_value() { v.into_int_value() } else { i64_type.const_int(0, false) };
                        let len_plus_1 = self.builder.build_int_add(len, i64_type.const_int(1, false), "").unwrap();
                        let buf_call = self.builder.build_call(malloc, &[len_plus_1.into()], "upper_buf").unwrap();
                        let buf = if let inkwell::values::ValueKind::Basic(v) = buf_call.try_as_basic_value() { v.into_pointer_value() } else { ptr_type.const_null() };

                        // Declare toupper
                        let i32_type = self.context.i32_type();
                        let toupper_ty = i32_type.fn_type(&[i32_type.into()], false);
                        let toupper = self.module.add_function("toupper", toupper_ty, None);

                        let idx_alloca = self.builder.build_alloca(i64_type, "up_i").unwrap();
                        self.builder.build_store(idx_alloca, i64_type.const_int(0, false)).unwrap();

                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let cond_bb = self.context.append_basic_block(function, "up_cond");
                        let body_bb = self.context.append_basic_block(function, "up_body");
                        let exit_bb = self.context.append_basic_block(function, "up_exit");

                        self.builder.build_unconditional_branch(cond_bb).unwrap();
                        self.builder.position_at_end(cond_bb);
                        let idx = self.builder.build_load(i64_type, idx_alloca, "i").unwrap().into_int_value();
                        let cmp = self.builder.build_int_compare(IntPredicate::SLT, idx, len, "cmp").unwrap();
                        self.builder.build_conditional_branch(cmp, body_bb, exit_bb).unwrap();

                        self.builder.position_at_end(body_bb);
                        let src_byte_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), src, &[idx], "sbp").unwrap() };
                        let byte = self.builder.build_load(self.context.i8_type(), src_byte_ptr, "byte").unwrap();
                        let byte_i32 = self.builder.build_int_z_extend(byte.into_int_value(), i32_type, "ext").unwrap();
                        let upper_call = self.builder.build_call(toupper, &[byte_i32.into()], "upper").unwrap();
                        let upper_byte = if let inkwell::values::ValueKind::Basic(v) = upper_call.try_as_basic_value() { v.into_int_value() } else { i32_type.const_int(0, false) };
                        let upper_i8 = self.builder.build_int_truncate(upper_byte, self.context.i8_type(), "trunc").unwrap();
                        let dst_byte_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), buf, &[idx], "dbp").unwrap() };
                        self.builder.build_store(dst_byte_ptr, upper_i8).unwrap();
                        let new_idx = self.builder.build_int_add(idx, i64_type.const_int(1, false), "").unwrap();
                        self.builder.build_store(idx_alloca, new_idx).unwrap();
                        self.builder.build_unconditional_branch(cond_bb).unwrap();

                        self.builder.position_at_end(exit_bb);
                        // Null-terminate
                        let end_ptr = unsafe { self.builder.build_gep(self.context.i8_type(), buf, &[len], "end").unwrap() };
                        self.builder.build_store(end_ptr, self.context.i8_type().const_int(0, false)).unwrap();
                        self.values.insert(result_id, buf.into());
                    }
                }
                _ => {
                    // Unknown stdlib call — skip
                }
            }
        }

        fn declare_printf(&mut self) {
            let i32_type = self.context.i32_type();
            let i64_type = self.context.i64_type();
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());

            let printf_type = i32_type.fn_type(&[ptr_type.into()], true);
            self.module.add_function("printf", printf_type, None);
            self.functions.insert("printf".into(), self.module.get_function("printf").unwrap());

            // malloc for heap string allocation
            let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
            self.module.add_function("malloc", malloc_type, None);
            self.functions.insert("malloc".into(), self.module.get_function("malloc").unwrap());

            // strlen
            let strlen_type = i64_type.fn_type(&[ptr_type.into()], false);
            self.module.add_function("strlen", strlen_type, None);
            self.functions.insert("strlen".into(), self.module.get_function("strlen").unwrap());

            // snprintf
            let snprintf_type = i32_type.fn_type(&[ptr_type.into(), i64_type.into(), ptr_type.into()], true);
            self.module.add_function("snprintf", snprintf_type, None);
            self.functions.insert("snprintf".into(), self.module.get_function("snprintf").unwrap());

            // strcpy
            let strcpy_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
            self.module.add_function("strcpy", strcpy_type, None);
            self.functions.insert("strcpy".into(), self.module.get_function("strcpy").unwrap());

            // strcat
            let strcat_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
            self.module.add_function("strcat", strcat_type, None);
            self.functions.insert("strcat".into(), self.module.get_function("strcat").unwrap());
        }

        fn dialect_to_llvm_type(&self, ty: &DialectType) -> Option<BasicMetadataTypeEnum<'ctx>> {
            match ty {
                DialectType::I64 => Some(self.context.i64_type().into()),
                DialectType::F64 => Some(self.context.f64_type().into()),
                DialectType::Bool => Some(self.context.bool_type().into()),
                DialectType::I32 => Some(self.context.i32_type().into()),
                DialectType::I8 => Some(self.context.i8_type().into()),
                DialectType::String => Some(self.context.ptr_type(inkwell::AddressSpace::default()).into()),
                DialectType::Named(name) => {
                    if self.struct_types.contains_key(name.as_str()) {
                        Some(self.context.ptr_type(inkwell::AddressSpace::default()).into())
                    } else {
                        None
                    }
                }
                DialectType::Result(_, _) | DialectType::Option(_) | DialectType::List(_) => {
                    Some(self.context.ptr_type(inkwell::AddressSpace::default()).into())
                }
                DialectType::Fn { .. } | DialectType::Closure { .. } => {
                    // Closure: { ptr fn_ptr, ptr env_ptr }
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    Some(self.context.struct_type(&[ptr_type.into(), ptr_type.into()], false).into())
                }
                _ => None,
            }
        }

        fn dialect_to_basic_type(&self, ty: &DialectType) -> Option<inkwell::types::BasicTypeEnum<'ctx>> {
            match ty {
                DialectType::I64 => Some(self.context.i64_type().into()),
                DialectType::F64 => Some(self.context.f64_type().into()),
                DialectType::Bool => Some(self.context.bool_type().into()),
                DialectType::I32 => Some(self.context.i32_type().into()),
                DialectType::I8 => Some(self.context.i8_type().into()),
                DialectType::String => Some(self.context.ptr_type(inkwell::AddressSpace::default()).into()),
                DialectType::Named(name) => {
                    if self.struct_types.contains_key(name.as_str()) {
                        Some(self.context.ptr_type(inkwell::AddressSpace::default()).into())
                    } else {
                        None
                    }
                }
                DialectType::Result(_, _) | DialectType::Option(_) | DialectType::List(_) => {
                    Some(self.context.ptr_type(inkwell::AddressSpace::default()).into())
                }
                DialectType::Fn { .. } | DialectType::Closure { .. } => {
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    Some(self.context.struct_type(&[ptr_type.into(), ptr_type.into()], false).into())
                }
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
            self.allocas.clear();

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
                OpKind::ConstString(s) => {
                    let global = self.builder.build_global_string_ptr(s, &format!("str_{}", result_id.0)).unwrap();
                    self.values.insert(result_id, global.as_pointer_value().into());
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
                                let inst = self.builder.build_float_add(lv, rv, "fadd").unwrap();
                                set_fast_math(inst);
                                Some(inst.into())
                            }
                            almide_ir::BinOp::SubFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                let inst = self.builder.build_float_sub(lv, rv, "fsub").unwrap();
                                set_fast_math(inst);
                                Some(inst.into())
                            }
                            almide_ir::BinOp::MulFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                let inst = self.builder.build_float_mul(lv, rv, "fmul").unwrap();
                                set_fast_math(inst);
                                Some(inst.into())
                            }
                            almide_ir::BinOp::DivFloat => {
                                let lv = l.into_float_value();
                                let rv = r.into_float_value();
                                let inst = self.builder.build_float_div(lv, rv, "fdiv").unwrap();
                                set_fast_math(inst);
                                Some(inst.into())
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
                            almide_ir::BinOp::ConcatStr => {
                                // String concatenation: malloc(strlen(a) + strlen(b) + 1), strcpy, strcat
                                let strlen = self.functions.get("strlen").copied().unwrap();
                                let malloc = self.functions.get("malloc").copied().unwrap();
                                let strcpy = self.functions.get("strcpy").copied().unwrap();
                                let strcat = self.functions.get("strcat").copied().unwrap();

                                let len_a = self.builder.build_call(strlen, &[l.into()], "len_a").unwrap();
                                let len_b = self.builder.build_call(strlen, &[r.into()], "len_b").unwrap();
                                let la = if let inkwell::values::ValueKind::Basic(v) = len_a.try_as_basic_value() { v.into_int_value() } else { self.context.i64_type().const_int(0, false) };
                                let lb = if let inkwell::values::ValueKind::Basic(v) = len_b.try_as_basic_value() { v.into_int_value() } else { self.context.i64_type().const_int(0, false) };
                                let total = self.builder.build_int_add(la, lb, "total_len").unwrap();
                                let total_plus_1 = self.builder.build_int_add(total, self.context.i64_type().const_int(1, false), "total_plus_1").unwrap();

                                let buf = self.builder.build_call(malloc, &[total_plus_1.into()], "buf").unwrap();
                                let buf_ptr = if let inkwell::values::ValueKind::Basic(v) = buf.try_as_basic_value() { v } else { l }; // fallback

                                self.builder.build_call(strcpy, &[buf_ptr.into(), l.into()], "").unwrap();
                                self.builder.build_call(strcat, &[buf_ptr.into(), r.into()], "").unwrap();

                                Some(buf_ptr)
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
                                BasicValueEnum::FloatValue(_) => self.builder.build_global_string_ptr("%.15g\n", "fmt_float").unwrap(),
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
                    } else if let Some((parent, tag, payload_tys)) = self.variant_cases.get(callee_str).cloned() {
                        // Variant constructor: malloc struct, set tag, store payload fields
                        if let Some(struct_ty) = self.struct_types.get(&parent).copied() {
                            let size = struct_ty.size_of().unwrap();
                            let malloc = self.functions.get("malloc").copied().unwrap();
                            let ptr_call = self.builder.build_call(malloc, &[size.into()], "variant_ptr").unwrap();
                            if let inkwell::values::ValueKind::Basic(ptr_val) = ptr_call.try_as_basic_value() {
                                let ptr = ptr_val.into_pointer_value();
                                // Store tag
                                let tag_ptr = self.builder.build_struct_gep(struct_ty, ptr, 0, "tag_ptr").unwrap();
                                self.builder.build_store(tag_ptr, self.context.i32_type().const_int(tag as u64, false)).unwrap();
                                // Store payload fields into the payload area
                                if !payload_tys.is_empty() {
                                    let payload_ptr = self.builder.build_struct_gep(struct_ty, ptr, 1, "payload_ptr").unwrap();
                                    for (i, arg_id) in args.iter().enumerate() {
                                        if let Some(arg_val) = self.values.get(arg_id) {
                                            let offset = (i * 8) as u64;
                                            let field_ptr = unsafe {
                                                self.builder.build_gep(self.context.i8_type(), payload_ptr, &[self.context.i64_type().const_int(offset, false)], &format!("pay_{}", i)).unwrap()
                                            };
                                            self.builder.build_store(field_ptr, *arg_val).unwrap();
                                        }
                                    }
                                }
                                self.values.insert(result_id, ptr.into());
                            }
                        }
                    } else if callee_str == "int.to_float" {
                        // int → float conversion
                        if let Some(val) = args.first().and_then(|a| self.values.get(a).cloned()) {
                            let f64_val = self.builder.build_signed_int_to_float(
                                val.into_int_value(), self.context.f64_type(), "itof").unwrap();
                            self.values.insert(result_id, f64_val.into());
                        }
                    } else if callee_str == "int.to_string" {
                        // Int → pass through to printf %lld
                        if let Some(val) = args.first().and_then(|a| self.values.get(a)) {
                            self.values.insert(result_id, *val);
                        }
                    } else if callee_str == "float.to_string" {
                        let fval = args.first().and_then(|a| self.values.get(a)).cloned();
                        if let Some(val) = fval {
                            let helper = self.get_or_emit_float_to_string();
                            let call = self.builder.build_call(helper, &[val.into()], "fts").unwrap();
                            if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                                self.values.insert(result_id, bv);
                            }
                        }
                    } else if callee_str.starts_with("list.") || callee_str.starts_with("string.") || callee_str.starts_with("map.") || callee_str.starts_with("math.") {
                        // Stdlib call — emit inline LLVM for list/string operations
                        let func_name = callee_str.split("__").next().unwrap_or(callee_str); // strip monomorph
                        let arg_vals: Vec<BasicValueEnum<'ctx>> = args.iter()
                            .filter_map(|a| self.values.get(a).cloned())
                            .collect();
                        self.emit_stdlib_call(result_id, func_name, &arg_vals, &op.result_ty);
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

                OpKind::ResultOkOp { value } | OpKind::OptionSomeOp { value } => {
                    let (type_name, tag) = if matches!(&op.kind, OpKind::ResultOkOp { .. }) {
                        ("Result", 0u32)
                    } else {
                        ("Option", 0u32)
                    };
                    if let Some(struct_ty) = self.struct_types.get(type_name).copied() {
                        let size = struct_ty.size_of().unwrap();
                        let malloc = self.functions.get("malloc").copied().unwrap();
                        let ptr_call = self.builder.build_call(malloc, &[size.into()], "ok_ptr").unwrap();
                        if let inkwell::values::ValueKind::Basic(ptr_val) = ptr_call.try_as_basic_value() {
                            let ptr = ptr_val.into_pointer_value();
                            let tag_ptr = self.builder.build_struct_gep(struct_ty, ptr, 0, "ok_tag").unwrap();
                            self.builder.build_store(tag_ptr, self.context.i32_type().const_int(tag as u64, false)).unwrap();
                            if let Some(val) = self.values.get(value) {
                                let payload_ptr = self.builder.build_struct_gep(struct_ty, ptr, 1, "ok_payload").unwrap();
                                self.builder.build_store(payload_ptr, *val).unwrap();
                            }
                            self.values.insert(result_id, ptr.into());
                        }
                    }
                }
                OpKind::ResultErrOp { value } => {
                    if let Some(struct_ty) = self.struct_types.get("Result").copied() {
                        let size = struct_ty.size_of().unwrap();
                        let malloc = self.functions.get("malloc").copied().unwrap();
                        let ptr_call = self.builder.build_call(malloc, &[size.into()], "err_ptr").unwrap();
                        if let inkwell::values::ValueKind::Basic(ptr_val) = ptr_call.try_as_basic_value() {
                            let ptr = ptr_val.into_pointer_value();
                            let tag_ptr = self.builder.build_struct_gep(struct_ty, ptr, 0, "err_tag").unwrap();
                            self.builder.build_store(tag_ptr, self.context.i32_type().const_int(1, false)).unwrap();
                            if let Some(val) = self.values.get(value) {
                                let payload_ptr = self.builder.build_struct_gep(struct_ty, ptr, 1, "err_payload").unwrap();
                                self.builder.build_store(payload_ptr, *val).unwrap();
                            }
                            self.values.insert(result_id, ptr.into());
                        }
                    }
                }
                OpKind::OptionNoneOp => {
                    if let Some(struct_ty) = self.struct_types.get("Option").copied() {
                        let size = struct_ty.size_of().unwrap();
                        let malloc = self.functions.get("malloc").copied().unwrap();
                        let ptr_call = self.builder.build_call(malloc, &[size.into()], "none_ptr").unwrap();
                        if let inkwell::values::ValueKind::Basic(ptr_val) = ptr_call.try_as_basic_value() {
                            let ptr = ptr_val.into_pointer_value();
                            let tag_ptr = self.builder.build_struct_gep(struct_ty, ptr, 0, "none_tag").unwrap();
                            self.builder.build_store(tag_ptr, self.context.i32_type().const_int(1, false)).unwrap();
                            self.values.insert(result_id, ptr.into());
                        }
                    }
                }

                OpKind::ListOp { elements } => {
                    // List layout: [i64 len][i64 elem0][i64 elem1]...
                    let i64_type = self.context.i64_type();
                    let malloc = self.functions.get("malloc").copied().unwrap();
                    let n = elements.len() as u64;
                    let total_bytes = 8 + n * 8; // 8 for len + 8 per element
                    let buf_call = self.builder.build_call(malloc, &[i64_type.const_int(total_bytes, false).into()], "list_ptr").unwrap();
                    if let inkwell::values::ValueKind::Basic(ptr_val) = buf_call.try_as_basic_value() {
                        let ptr = ptr_val.into_pointer_value();
                        // Store length
                        self.builder.build_store(ptr, i64_type.const_int(n, false)).unwrap();
                        // Store elements
                        for (i, elem_id) in elements.iter().enumerate() {
                            if let Some(elem_val) = self.values.get(elem_id) {
                                let offset = (8 + i * 8) as u64;
                                let elem_ptr = unsafe {
                                    self.builder.build_gep(self.context.i8_type(), ptr, &[i64_type.const_int(offset, false)], &format!("elem_{}", i)).unwrap()
                                };
                                self.builder.build_store(elem_ptr, *elem_val).unwrap();
                            }
                        }
                        self.values.insert(result_id, ptr.into());
                    }
                }

                OpKind::MapOp { .. } | OpKind::EmptyMapOp => {
                    // Map layout: [i64 len][pairs...] - for now just store length
                    let i64_type = self.context.i64_type();
                    let malloc = self.functions.get("malloc").copied().unwrap();
                    let n = if let OpKind::MapOp { entries } = &op.kind { entries.len() } else { 0 };
                    let total_bytes = 8 + n * 16; // 8 for len + 16 per entry (key ptr + val i64)
                    let buf_call = self.builder.build_call(malloc, &[i64_type.const_int(total_bytes as u64, false).into()], "map_ptr").unwrap();
                    if let inkwell::values::ValueKind::Basic(ptr_val) = buf_call.try_as_basic_value() {
                        let ptr = ptr_val.into_pointer_value();
                        self.builder.build_store(ptr, i64_type.const_int(n as u64, false)).unwrap();
                        // Store entries (key-value pairs) for future use
                        if let OpKind::MapOp { entries } = &op.kind {
                            for (i, (k, v)) in entries.iter().enumerate() {
                                let offset_k = (8 + i * 16) as u64;
                                let offset_v = (8 + i * 16 + 8) as u64;
                                if let Some(kv) = self.values.get(k) {
                                    let kp = unsafe { self.builder.build_gep(self.context.i8_type(), ptr, &[i64_type.const_int(offset_k, false)], "mk").unwrap() };
                                    self.builder.build_store(kp, *kv).unwrap();
                                }
                                if let Some(vv) = self.values.get(v) {
                                    let vp = unsafe { self.builder.build_gep(self.context.i8_type(), ptr, &[i64_type.const_int(offset_v, false)], "mv").unwrap() };
                                    self.builder.build_store(vp, *vv).unwrap();
                                }
                            }
                        }
                        self.values.insert(result_id, ptr.into());
                    }
                }

                OpKind::RecordOp { name, fields } => {
                    if let Some(type_name) = name {
                        if let Some(struct_ty) = self.struct_types.get(type_name.as_str()).copied() {
                            // Allocate struct on heap
                            let size = struct_ty.size_of().unwrap();
                            let malloc = self.functions.get("malloc").copied().unwrap();
                            let ptr = self.builder.build_call(malloc, &[size.into()], "record_ptr").unwrap();
                            if let inkwell::values::ValueKind::Basic(ptr_val) = ptr.try_as_basic_value() {
                                let ptr_val = ptr_val.into_pointer_value();
                                // Store each field
                                if let Some(field_names) = self.struct_fields.get(type_name.as_str()) {
                                    for (fname, fval_id) in fields {
                                        if let Some(idx) = field_names.iter().position(|n| n == fname.as_str()) {
                                            if let Some(fval) = self.values.get(fval_id) {
                                                let field_ptr = self.builder.build_struct_gep(struct_ty, ptr_val, idx as u32, &format!("field_{}", fname)).unwrap();
                                                self.builder.build_store(field_ptr, *fval).unwrap();
                                            }
                                        }
                                    }
                                }
                                self.values.insert(result_id, ptr_val.into());
                            }
                        }
                    }
                }
                OpKind::MemberOp { object, field } => {
                    // Field access on a record pointer
                    if let Some(obj_val) = self.values.get(object).cloned() {
                        let ptr = obj_val.into_pointer_value();
                        // Find struct type from the object's type info
                        // We need to find which struct this pointer points to.
                        // Look through all struct types for a matching field.
                        let mut found = false;
                        for (type_name, fields) in &self.struct_fields {
                            if let Some(idx) = fields.iter().position(|n| n == field.as_str()) {
                                if let Some(struct_ty) = self.struct_types.get(type_name.as_str()).copied() {
                                    let field_ptr = self.builder.build_struct_gep(struct_ty, ptr, idx as u32, &format!("get_{}", field)).unwrap();
                                    // Determine field type
                                    let field_ty = struct_ty.get_field_type_at_index(idx as u32).unwrap();
                                    let loaded = self.builder.build_load(field_ty, field_ptr, &format!("load_{}", field)).unwrap();
                                    self.values.insert(result_id, loaded);
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                OpKind::IndexOp { object, index } => {
                    // List indexing: list_ptr + 8 + index * 8
                    if let (Some(list_val), Some(idx_val)) = (self.values.get(object).cloned(), self.values.get(index).cloned()) {
                        let i64_type = self.context.i64_type();
                        let lp = list_val.into_pointer_value();
                        let idx = idx_val.into_int_value();
                        let offset = self.builder.build_int_add(
                            self.builder.build_int_mul(idx, i64_type.const_int(8, false), "").unwrap(),
                            i64_type.const_int(8, false), "off").unwrap();
                        let elem_ptr = unsafe {
                            self.builder.build_gep(self.context.i8_type(), lp, &[offset], "idx_ptr").unwrap()
                        };
                        // Determine element type from result_ty
                        let load_ty = self.dialect_to_basic_type(&op.result_ty).unwrap_or(i64_type.into());
                        let loaded = self.builder.build_load(load_ty, elem_ptr, "idx_val").unwrap();
                        self.values.insert(result_id, loaded);
                    }
                }

                OpKind::MatchOp { subject, arms } => {
                    if let Some(subj_val) = self.values.get(subject).cloned() {
                        let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                        let merge_bb = self.context.append_basic_block(function, "match.merge");

                        // Pre-create all arm blocks
                        let arm_bbs: Vec<_> = arms.iter().enumerate()
                            .map(|(i, _)| self.context.append_basic_block(function, &format!("match.arm{}", i)))
                            .collect();

                        // Determine if this is a variant match (subject is ptr to tagged union)
                        // or an integer match.
                        let is_variant_match = arms.iter().any(|a| matches!(&a.pattern, crate::ops::MatchPattern::Variant { .. }));

                        let switch_val = if is_variant_match {
                            // Load tag from variant struct: GEP + load i32
                            let ptr = subj_val.into_pointer_value();
                            // Find the variant type
                            let variant_ty = self.struct_types.values().next().copied(); // TODO: track per-value type
                            if let Some(sty) = variant_ty {
                                let tag_ptr = self.builder.build_struct_gep(sty, ptr, 0, "match_tag_ptr").unwrap();
                                self.builder.build_load(self.context.i32_type(), tag_ptr, "match_tag").unwrap().into_int_value()
                            } else {
                                self.context.i32_type().const_int(0, false)
                            }
                        } else {
                            subj_val.into_int_value()
                        };

                        let mut cases: Vec<(inkwell::values::IntValue<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)> = Vec::new();
                        // Last arm is always default (wildcard or last variant)
                        let default_bb = *arm_bbs.last().unwrap_or(&merge_bb);
                        for (i, arm) in arms.iter().enumerate() {
                            // Skip the default arm (last one) — it's the switch default target
                            if arm_bbs[i] == default_bb && !matches!(&arm.pattern, crate::ops::MatchPattern::LitInt(_) | crate::ops::MatchPattern::Variant { .. }) {
                                continue;
                            }
                            match &arm.pattern {
                                crate::ops::MatchPattern::LitInt(v) => {
                                    let const_ty = if is_variant_match { self.context.i32_type() } else { self.context.i64_type() };
                                    cases.push((const_ty.const_int(*v as u64, true), arm_bbs[i]));
                                }
                                crate::ops::MatchPattern::Variant { tag, .. } => {
                                    if let Some((_, tag_idx, _)) = self.variant_cases.get(tag.as_str()) {
                                        cases.push((self.context.i32_type().const_int(*tag_idx as u64, false), arm_bbs[i]));
                                    }
                                }
                                _ => {} // wildcard/binding handled as default
                            }
                        }
                        self.builder.build_switch(switch_val, default_bb, &cases).unwrap();

                        // Compile each arm body
                        let mut arm_results: Vec<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)> = Vec::new();
                        for (i, arm) in arms.iter().enumerate() {
                            self.builder.position_at_end(arm_bbs[i]);

                            // For variant patterns, extract payload bindings
                            if let crate::ops::MatchPattern::Variant { tag, bindings } = &arm.pattern {
                                if let Some((parent, _, payload_tys)) = self.variant_cases.get(tag.as_str()).cloned() {
                                    if let Some(sty) = self.struct_types.get(&parent).copied() {
                                        let ptr = subj_val.into_pointer_value();
                                        let payload_ptr = self.builder.build_struct_gep(sty, ptr, 1, "arm_payload").unwrap();
                                        for (j, binding_id) in bindings.iter().enumerate() {
                                            let offset = (j * 8) as u64;
                                            let field_ptr = unsafe {
                                                self.builder.build_gep(self.context.i8_type(), payload_ptr, &[self.context.i64_type().const_int(offset, false)], &format!("bind_{}", j)).unwrap()
                                            };
                                            let field_ty = if j < payload_tys.len() {
                                                self.dialect_to_basic_type(&payload_tys[j]).unwrap_or(self.context.i64_type().into())
                                            } else {
                                                self.context.i64_type().into()
                                            };
                                            let loaded = self.builder.build_load(field_ty, field_ptr, &format!("payload_{}", j)).unwrap();
                                            self.values.insert(*binding_id, loaded);
                                        }
                                    }
                                }
                            }

                            let mut arm_val = None;
                            for block in &arm.body {
                                for op in &block.ops { self.compile_op(op); }
                                if let Terminator::Yield(v) = &block.terminator {
                                    arm_val = self.values.get(v).cloned();
                                }
                            }
                            self.builder.build_unconditional_branch(merge_bb).unwrap();
                            if let Some(val) = arm_val {
                                arm_results.push((val, self.builder.get_insert_block().unwrap()));
                            }
                        }

                        // Phi at merge
                        self.builder.position_at_end(merge_bb);
                        if !arm_results.is_empty() {
                            let phi = self.builder.build_phi(arm_results[0].0.get_type(), "match_val").unwrap();
                            let incoming: Vec<(&dyn inkwell::values::BasicValue, inkwell::basic_block::BasicBlock)> =
                                arm_results.iter().map(|(v, bb)| (v as &dyn inkwell::values::BasicValue, *bb)).collect();
                            phi.add_incoming(&incoming);
                            self.values.insert(result_id, phi.as_basic_value());
                        }
                    }
                }

                OpKind::AllocVar { init, ty } => {
                    if let Some(llvm_ty) = self.dialect_to_basic_type(ty) {
                        let alloca = self.builder.build_alloca(llvm_ty, &format!("var_{}", result_id.0)).unwrap();
                        if let Some(init_val) = self.values.get(init) {
                            self.builder.build_store(alloca, *init_val).unwrap();
                        }
                        // Store the alloca pointer as the value for this slot
                        self.allocas.insert(result_id, alloca);
                        self.values.insert(result_id, alloca.into());
                    }
                }
                OpKind::LoadVar { slot } => {
                    if let Some(alloca) = self.allocas.get(slot).copied() {
                        let pointee = self.dialect_to_basic_type(&op.result_ty)
                            .unwrap_or(self.context.i64_type().into());
                        let loaded = self.builder.build_load(pointee, alloca, &format!("load_{}", result_id.0)).unwrap();
                        self.values.insert(result_id, loaded);
                    }
                }
                OpKind::StoreVar { slot, value } => {
                    if let (Some(alloca), Some(val)) = (self.allocas.get(slot).copied(), self.values.get(value)) {
                        self.builder.build_store(alloca, *val).unwrap();
                    }
                }

                OpKind::WhileOp { cond_region, body } => {
                    let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                    let cond_bb = self.context.append_basic_block(function, "while.cond");
                    let body_bb = self.context.append_basic_block(function, "while.body");
                    let exit_bb = self.context.append_basic_block(function, "while.exit");

                    self.builder.build_unconditional_branch(cond_bb).unwrap();

                    // Condition
                    self.builder.position_at_end(cond_bb);
                    let mut cond_val = None;
                    for block in cond_region {
                        for op in &block.ops { self.compile_op(op); }
                        if let Terminator::Yield(v) = &block.terminator {
                            cond_val = self.values.get(v).cloned();
                        }
                    }
                    if let Some(cv) = cond_val {
                        self.builder.build_conditional_branch(cv.into_int_value(), body_bb, exit_bb).unwrap();
                    } else {
                        self.builder.build_unconditional_branch(exit_bb).unwrap();
                    }

                    // Body
                    self.builder.position_at_end(body_bb);
                    for block in body {
                        for op in &block.ops { self.compile_op(op); }
                    }
                    self.builder.build_unconditional_branch(cond_bb).unwrap();

                    // Continue after loop
                    self.builder.position_at_end(exit_bb);
                }

                OpKind::LambdaOp { params, body } => {
                    // Lift lambda to a global function.
                    // Closure representation: { fn_ptr, env_ptr }
                    // For non-capturing lambdas, env_ptr is null.
                    let lambda_name = format!("__lambda_{}", result_id.0);
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());

                    // Build lambda function type: (ptr env, params...) -> ret
                    let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = vec![ptr_type.into()]; // env ptr
                    for (_, dty) in params {
                        if let Some(ty) = self.dialect_to_llvm_type(dty) {
                            param_types.push(ty);
                        }
                    }

                    let ret_ty = if let Some(block) = body.first() {
                        // Infer return type from body's result type
                        block.ops.last().map(|op| self.dialect_to_basic_type(&op.result_ty)).flatten()
                    } else {
                        None
                    };

                    let fn_type = if let Some(ret) = ret_ty {
                        ret.fn_type(&param_types, false)
                    } else {
                        self.context.void_type().fn_type(&param_types, false)
                    };

                    let lambda_fn = self.module.add_function(&lambda_name, fn_type, None);
                    let lambda_entry = self.context.append_basic_block(lambda_fn, "entry");

                    // Detect captured variables: ValueIds used in body but not defined in body or params
                    let param_ids: std::collections::HashSet<ValueId> = params.iter().map(|(v, _)| *v).collect();
                    let mut body_defined: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
                    let mut body_used: Vec<ValueId> = Vec::new();
                    for block in body {
                        for bop in &block.ops {
                            if let Some(r) = bop.result { body_defined.insert(r); }
                            // Collect used ValueIds from each op kind
                            match &bop.kind {
                                OpKind::BinOp { lhs, rhs, .. } => { body_used.push(*lhs); body_used.push(*rhs); }
                                OpKind::UnOp { operand, .. } => { body_used.push(*operand); }
                                OpKind::CallOp { args, .. } | OpKind::IntrinsicCallOp { args, .. } | OpKind::ComputedCallOp { args, .. } => {
                                    body_used.extend(args);
                                }
                                _ => {}
                            }
                        }
                    }
                    let captures: Vec<(ValueId, BasicValueEnum<'ctx>)> = body_used.iter()
                        .filter(|v| !param_ids.contains(v) && !body_defined.contains(v))
                        .filter_map(|v| self.values.get(v).map(|val| (*v, *val)))
                        .collect::<Vec<_>>();
                    // Deduplicate
                    let mut seen_captures = std::collections::HashSet::new();
                    let captures: Vec<(ValueId, BasicValueEnum<'ctx>)> = captures.into_iter()
                        .filter(|(v, _)| seen_captures.insert(*v))
                        .collect();

                    // Save state
                    let saved_block = self.builder.get_insert_block();
                    let saved_values = self.values.clone();

                    self.builder.position_at_end(lambda_entry);

                    // Map params
                    for (i, (val_id, _)) in params.iter().enumerate() {
                        if let Some(param) = lambda_fn.get_nth_param((i + 1) as u32) {
                            self.values.insert(*val_id, param);
                        }
                    }

                    // Unpack captures from env
                    if !captures.is_empty() {
                        let env_ptr = lambda_fn.get_nth_param(0).unwrap().into_pointer_value();
                        for (i, (cap_id, cap_val)) in captures.iter().enumerate() {
                            let offset = (i * 8) as u64;
                            let field_ptr = unsafe {
                                self.builder.build_gep(self.context.i8_type(), env_ptr, &[self.context.i64_type().const_int(offset, false)], &format!("cap_{}", i)).unwrap()
                            };
                            let loaded = self.builder.build_load(cap_val.get_type(), field_ptr, &format!("cap_load_{}", i)).unwrap();
                            self.values.insert(*cap_id, loaded);
                        }
                    }

                    // Compile body
                    let mut last_val = None;
                    for block in body {
                        for bop in &block.ops { self.compile_op(bop); }
                        if let Terminator::Yield(v) | Terminator::Return(v) = &block.terminator {
                            last_val = self.values.get(v).cloned();
                        }
                    }
                    if let Some(val) = last_val {
                        self.builder.build_return(Some(&val)).unwrap();
                    } else {
                        self.builder.build_return(None).unwrap();
                    }

                    // Restore state
                    self.values = saved_values;
                    if let Some(bb) = saved_block {
                        self.builder.position_at_end(bb);
                    }

                    // Create env struct on heap (if captures exist)
                    let env_ptr_val = if captures.is_empty() {
                        ptr_type.const_null()
                    } else {
                        let env_size = (captures.len() * 8) as u64;
                        let malloc = self.functions.get("malloc").copied().unwrap();
                        let env_call = self.builder.build_call(malloc, &[self.context.i64_type().const_int(env_size, false).into()], "env_alloc").unwrap();
                        let env_ptr = if let inkwell::values::ValueKind::Basic(v) = env_call.try_as_basic_value() {
                            v.into_pointer_value()
                        } else {
                            ptr_type.const_null()
                        };
                        // Store captures
                        for (i, (_, cap_val)) in captures.iter().enumerate() {
                            let offset = (i * 8) as u64;
                            let field_ptr = unsafe {
                                self.builder.build_gep(self.context.i8_type(), env_ptr, &[self.context.i64_type().const_int(offset, false)], &format!("env_store_{}", i)).unwrap()
                            };
                            self.builder.build_store(field_ptr, *cap_val).unwrap();
                        }
                        env_ptr
                    };

                    // Create closure struct: { fn_ptr, env_ptr }
                    let closure_ty = self.context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
                    let closure_alloca = self.builder.build_alloca(closure_ty, "closure").unwrap();
                    let fn_ptr = lambda_fn.as_global_value().as_pointer_value();
                    let fn_field = self.builder.build_struct_gep(closure_ty, closure_alloca, 0, "fn_ptr").unwrap();
                    self.builder.build_store(fn_field, fn_ptr).unwrap();
                    let env_field = self.builder.build_struct_gep(closure_ty, closure_alloca, 1, "env_ptr").unwrap();
                    self.builder.build_store(env_field, env_ptr_val).unwrap();

                    let closure_val = self.builder.build_load(closure_ty, closure_alloca, "closure_val").unwrap();
                    self.values.insert(result_id, closure_val);
                }

                OpKind::ComputedCallOp { callee, args } => {
                    // Call a closure: extract fn_ptr and env_ptr, call fn_ptr(env_ptr, args...)
                    if let Some(closure_val) = self.values.get(callee).cloned() {
                        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                        let closure_ty = self.context.struct_type(&[ptr_type.into(), ptr_type.into()], false);

                        // Extract fn_ptr and env_ptr
                        let fn_ptr = self.builder.build_extract_value(closure_val.into_struct_value(), 0, "fn_ptr").unwrap();
                        let env_ptr = self.builder.build_extract_value(closure_val.into_struct_value(), 1, "env_ptr").unwrap();

                        // Build call: fn_ptr(env_ptr, args...)
                        let mut call_args: Vec<BasicMetadataValueEnum> = vec![env_ptr.into()];
                        for a in args {
                            if let Some(v) = self.values.get(a) {
                                call_args.push((*v).into());
                            }
                        }

                        // Build function type from argument types
                        let arg_types: Vec<BasicMetadataTypeEnum> = call_args.iter().map(|a| {
                            match a {
                                BasicMetadataValueEnum::IntValue(v) => v.get_type().into(),
                                BasicMetadataValueEnum::FloatValue(v) => v.get_type().into(),
                                BasicMetadataValueEnum::PointerValue(v) => v.get_type().into(),
                                _ => self.context.i64_type().into(),
                            }
                        }).collect();
                        let ret_basic = self.dialect_to_basic_type(&op.result_ty);
                        let fn_type = if let Some(ret) = ret_basic {
                            ret.fn_type(&arg_types, false)
                        } else {
                            self.context.void_type().fn_type(&arg_types, false)
                        };

                        let call = self.builder.build_indirect_call(fn_type, fn_ptr.into_pointer_value(), &call_args, "closure_call").unwrap();
                        if let inkwell::values::ValueKind::Basic(bv) = call.try_as_basic_value() {
                            self.values.insert(result_id, bv);
                        }
                    }
                }

                _ => {} // TODO: for, collections, etc.
            }
        }
    }
}
