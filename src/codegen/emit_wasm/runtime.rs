//! WASM runtime functions: bump allocator, println, int_to_string.
//!
//! These are emitted as regular WASM functions, not imports.
//! Only fd_write is imported from WASI.

use super::{CompiledFunc, WasmEmitter, SCRATCH_ITOA, NEWLINE_OFFSET};
use wasm_encoder::{BlockType, Function, Instruction, MemArg, ValType};

fn mem(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

fn mem8(offset: u64) -> MemArg {
    MemArg { offset, align: 0, memory_index: 0 }
}

/// Register WASI imports and runtime function signatures.
pub fn register_runtime(emitter: &mut WasmEmitter) {
    // fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    let fd_write_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_write = emitter.register_import(fd_write_ty);

    // __alloc(size: i32) -> i32
    let alloc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.alloc = emitter.register_func("__alloc", alloc_ty);

    // __println_str(ptr: i32) -> ()
    let println_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.println_str = emitter.register_func("__println_str", println_ty);

    // __int_to_string(n: i64) -> i32
    let itoa_ty = emitter.register_type(vec![ValType::I64], vec![ValType::I32]);
    emitter.rt.int_to_string = emitter.register_func("__int_to_string", itoa_ty);

    // __println_int(n: i64) -> ()
    let println_int_ty = emitter.register_type(vec![ValType::I64], vec![]);
    emitter.rt.println_int = emitter.register_func("__println_int", println_int_ty);

    // __concat_str(left: i32, right: i32) -> i32
    let concat_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.concat_str = emitter.register_func("__concat_str", concat_ty);

    // __str_eq(a: i32, b: i32) -> i32
    let str_eq_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.str_eq = emitter.register_func("__str_eq", str_eq_ty);

    // __mem_eq(a: i32, b: i32, size: i32) -> i32
    let mem_eq_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.mem_eq = emitter.register_func("__mem_eq", mem_eq_ty);

    // __list_eq(a: i32, b: i32, elem_size: i32) -> i32
    let list_eq_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.list_eq = emitter.register_func("__list_eq", list_eq_ty);

    // __concat_list(a: i32, b: i32, elem_size: i32) -> i32
    let concat_list_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.concat_list = emitter.register_func("__concat_list", concat_list_ty);

    // Global: __heap_ptr (mutable i32, initialized at assembly time)
    emitter.heap_ptr_global = 0; // first and only global
}

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_println_str(emitter);
    compile_int_to_string(emitter);
    compile_println_int(emitter);
    compile_concat_str(emitter);
    compile_str_eq(emitter);
    compile_mem_eq(emitter);
    compile_list_eq(emitter);
    compile_concat_list(emitter);
}

/// __alloc(size: i32) -> i32
/// Bump allocator: returns current heap_ptr, then advances it by size.
fn compile_alloc(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc];
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $ptr

    // $ptr = global.__heap_ptr
    f.instruction(&Instruction::GlobalGet(emitter.heap_ptr_global));
    f.instruction(&Instruction::LocalSet(1));

    // global.__heap_ptr += size
    f.instruction(&Instruction::GlobalGet(emitter.heap_ptr_global));
    f.instruction(&Instruction::LocalGet(0)); // param $size
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::GlobalSet(emitter.heap_ptr_global));

    // return $ptr
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::End);

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __println_str(ptr: i32)
/// Prints string at ptr ([len:i32][data:u8...]) followed by newline via WASI fd_write.
fn compile_println_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_str];
    let mut f = Function::new([]);

    // --- Write the string ---
    // iov[0].buf = ptr + 4  (skip length prefix)
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalGet(0)); // param $ptr
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Store(mem(0)));

    // iov[0].len = *ptr  (load length)
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::I32Store(mem(0)));

    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Const(8));
    f.instruction(&Instruction::Call(emitter.rt.fd_write));
    f.instruction(&Instruction::Drop);

    // --- Write newline ---
    // iov[0].buf = NEWLINE_OFFSET
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32Const(NEWLINE_OFFSET as i32));
    f.instruction(&Instruction::I32Store(mem(0)));

    // iov[0].len = 1
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Store(mem(0)));

    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Const(8));
    f.instruction(&Instruction::Call(emitter.rt.fd_write));
    f.instruction(&Instruction::Drop);

    f.instruction(&Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __int_to_string(n: i64) -> i32
/// Converts an i64 to a decimal string on the heap.
/// Uses scratch area [SCRATCH_ITOA..SCRATCH_ITOA+32) for digit buffer.
fn compile_int_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_to_string];
    // Locals: 0=$n (param), 1=$pos, 2=$is_neg, 3=$abs_n(i64), 4=$start, 5=$len, 6=$result, 7=$i
    let mut f = Function::new([
        (1, ValType::I32),  // 1: $pos
        (1, ValType::I32),  // 2: $is_neg
        (1, ValType::I64),  // 3: $abs_n
        (1, ValType::I32),  // 4: $start
        (1, ValType::I32),  // 5: $len
        (1, ValType::I32),  // 6: $result
        (1, ValType::I32),  // 7: $i
    ]);

    let scratch_end = SCRATCH_ITOA + 31;

    // $pos = scratch_end (write backwards from end of scratch buffer)
    f.instruction(&Instruction::I32Const(scratch_end as i32));
    f.instruction(&Instruction::LocalSet(1));

    // $is_neg = $n < 0
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I64Const(0));
    f.instruction(&Instruction::I64LtS);
    f.instruction(&Instruction::LocalSet(2));

    // $abs_n = if $is_neg then -$n else $n
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
    f.instruction(&Instruction::I64Const(0));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I64Sub);
    f.instruction(&Instruction::Else);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::LocalSet(3));

    // if $abs_n == 0: write '0'
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I64Eqz);
    f.instruction(&Instruction::If(BlockType::Empty));
    {
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Const(48)); // '0'
        f.instruction(&Instruction::I32Store8(mem8(0)));
        // $pos -= 1
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalSet(1));
    }
    f.instruction(&Instruction::Else);
    {
        // while $abs_n > 0: write digits backwards
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        {
            // break if $abs_n == 0
            f.instruction(&Instruction::LocalGet(3));
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::BrIf(1)); // break outer block

            // mem[$pos] = ($abs_n % 10) + '0'
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::LocalGet(3));
            f.instruction(&Instruction::I64Const(10));
            f.instruction(&Instruction::I64RemS);
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::I32Const(48));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::I32Store8(mem8(0)));

            // $pos -= 1
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Sub);
            f.instruction(&Instruction::LocalSet(1));

            // $abs_n /= 10
            f.instruction(&Instruction::LocalGet(3));
            f.instruction(&Instruction::I64Const(10));
            f.instruction(&Instruction::I64DivS);
            f.instruction(&Instruction::LocalSet(3));

            f.instruction(&Instruction::Br(0)); // continue loop
        }
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block
    }
    f.instruction(&Instruction::End); // end if/else

    // if $is_neg: write '-'
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::If(BlockType::Empty));
    {
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Const(45)); // '-'
        f.instruction(&Instruction::I32Store8(mem8(0)));
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalSet(1));
    }
    f.instruction(&Instruction::End);

    // $start = $pos + 1
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(4));

    // $len = scratch_end - $pos
    f.instruction(&Instruction::I32Const(scratch_end as i32));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Sub);
    f.instruction(&Instruction::LocalSet(5));

    // $result = __alloc(4 + $len)
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::Call(emitter.rt.alloc));
    f.instruction(&Instruction::LocalSet(6));

    // mem32[$result] = $len
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Store(mem(0)));

    // memcpy: copy $len bytes from $start to $result+4
    // Using a byte-by-byte copy loop ($i = 0; while $i < $len)
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(7));

    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    {
        // break if $i >= $len
        f.instruction(&Instruction::LocalGet(7));
        f.instruction(&Instruction::LocalGet(5));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        // mem[$result + 4 + $i] = mem[$start + $i]
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(7));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(4));
        f.instruction(&Instruction::LocalGet(7));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load8U(mem8(0)));

        f.instruction(&Instruction::I32Store8(mem8(0)));

        // $i += 1
        f.instruction(&Instruction::LocalGet(7));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(7));

        f.instruction(&Instruction::Br(0)); // continue
    }
    f.instruction(&Instruction::End); // end loop
    f.instruction(&Instruction::End); // end block

    // return $result
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::End);

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __println_int(n: i64)
/// Convenience: int_to_string then println_str.
fn compile_println_int(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_int];
    let mut f = Function::new([]);

    f.instruction(&Instruction::LocalGet(0)); // param $n
    f.instruction(&Instruction::Call(emitter.rt.int_to_string));
    f.instruction(&Instruction::Call(emitter.rt.println_str));
    f.instruction(&Instruction::End);

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __concat_str(left: i32, right: i32) -> i32
/// Concatenates two strings. Each is [len:i32][data:u8...].
/// Returns a new heap-allocated string.
fn compile_concat_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.concat_str];
    // params: 0=$left, 1=$right
    // locals: 2=$left_len, 3=$right_len, 4=$new_len, 5=$result, 6=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $left_len
        (1, ValType::I32), // 3: $right_len
        (1, ValType::I32), // 4: $new_len
        (1, ValType::I32), // 5: $result
        (1, ValType::I32), // 6: $i
    ]);

    // $left_len = mem32[$left]
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(2));

    // $right_len = mem32[$right]
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(3));

    // $new_len = $left_len + $right_len
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(4));

    // $result = __alloc(4 + $new_len)
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::Call(emitter.rt.alloc));
    f.instruction(&Instruction::LocalSet(5));

    // mem32[$result] = $new_len
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Store(mem(0)));

    // Copy left data: memcpy($result+4, $left+4, $left_len)
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(6));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    {
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        f.instruction(&Instruction::LocalGet(5));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load8U(mem8(0)));
        f.instruction(&Instruction::I32Store8(mem8(0)));

        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(6));
        f.instruction(&Instruction::Br(0));
    }
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);

    // Copy right data: memcpy($result+4+$left_len, $right+4, $right_len)
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(6));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    {
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        f.instruction(&Instruction::LocalGet(5));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load8U(mem8(0)));
        f.instruction(&Instruction::I32Store8(mem8(0)));

        f.instruction(&Instruction::LocalGet(6));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(6));
        f.instruction(&Instruction::Br(0));
    }
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);

    // return $result
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::End);

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __str_eq(a: i32, b: i32) -> i32
/// Deep string equality: compare lengths then bytes. Returns 1 if equal.
fn compile_str_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.str_eq];
    // params: 0=$a, 1=$b. locals: 2=$len_a, 3=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $len_a
        (1, ValType::I32), // 3: $i
    ]);

    // If same pointer, return 1
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // Load a.len
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(2));

    // If lengths differ, return 0
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // Compare bytes: for i = 0; i < len; i++
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(3));

    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));

    // if $i >= $len → all bytes matched, return 1
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // if a[4+i] != b[4+i] → return 0
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // $i += 1, continue loop
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::Br(0));

    f.instruction(&Instruction::End); // end loop
    f.instruction(&Instruction::End); // end block

    // Fallback (shouldn't reach here)
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __mem_eq(a: i32, b: i32, size: i32) -> i32
/// Byte-by-byte comparison of two memory regions.
fn compile_mem_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.mem_eq];
    // params: 0=$a, 1=$b, 2=$size. locals: 3=$i
    let mut f = Function::new([(1, ValType::I32)]);

    // Same pointer → equal
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // Compare bytes
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));

    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::Br(0));

    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __list_eq(a: i32, b: i32, elem_size: i32) -> i32
/// Compare two lists byte-by-byte. Returns 1 if equal.
fn compile_list_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.list_eq];
    // params: 0=$a, 1=$b, 2=$elem_size. locals: 3=$len, 4=$total_bytes, 5=$i
    let mut f = Function::new([
        (1, ValType::I32), // 3: $len
        (1, ValType::I32), // 4: $total_bytes
        (1, ValType::I32), // 5: $i
    ]);

    // Same pointer → equal
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // Compare lengths
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(3));

    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // $total_bytes = $len * $elem_size
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Mul);
    f.instruction(&Instruction::LocalSet(4));

    // Byte-by-byte comparison of data section
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(5));

    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));

    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    // Compare a[4+i] vs b[4+i]
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);

    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(5));
    f.instruction(&Instruction::Br(0));

    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block

    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __concat_list(a: i32, b: i32, elem_size: i32) -> i32
/// Concatenate two lists. Layout: [len:i32][data...]. Generic over elem_size.
fn compile_concat_list(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.concat_list];
    // params: 0=$a, 1=$b, 2=$elem_size
    // locals: 3=$len_a, 4=$len_b, 5=$new_len, 6=$result, 7=$bytes_a, 8=$bytes_b, 9=$i
    let mut f = Function::new([
        (1, ValType::I32), // 3: $len_a
        (1, ValType::I32), // 4: $len_b
        (1, ValType::I32), // 5: $new_len
        (1, ValType::I32), // 6: $result
        (1, ValType::I32), // 7: $bytes_a
        (1, ValType::I32), // 8: $bytes_b
        (1, ValType::I32), // 9: $i
    ]);

    // $len_a = mem32[$a]
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(3));

    // $len_b = mem32[$b]
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(0)));
    f.instruction(&Instruction::LocalSet(4));

    // $new_len = $len_a + $len_b
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(5));

    // $bytes_a = $len_a * $elem_size
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Mul);
    f.instruction(&Instruction::LocalSet(7));

    // $bytes_b = $len_b * $elem_size
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Mul);
    f.instruction(&Instruction::LocalSet(8));

    // $result = alloc(4 + $bytes_a + $bytes_b)
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::LocalGet(7));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(8));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::Call(emitter.rt.alloc));
    f.instruction(&Instruction::LocalSet(6));

    // mem32[$result] = $new_len
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Store(mem(0)));

    // Copy a's data: byte-by-byte from $a+4 to $result+4, $bytes_a bytes
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(9));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::LocalGet(7));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // dst
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Add);
    // src
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::I32Store8(mem8(0)));
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(9));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);

    // Copy b's data: from $b+4 to $result+4+$bytes_a, $bytes_b bytes
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(9));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::LocalGet(8));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // dst
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(7));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Add);
    // src
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem8(0)));
    f.instruction(&Instruction::I32Store8(mem8(0)));
    f.instruction(&Instruction::LocalGet(9));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(9));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);

    // return $result
    f.instruction(&Instruction::LocalGet(6));
    f.instruction(&Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
