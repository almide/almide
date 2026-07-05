pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countIncs_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_2) {
        0 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            x_9 = lean_ctor_get(x_2, 0);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_2, 1);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_2, 2);
            lean_inc(x_11);
            lean_dec(x_2);
            x_12 = lean_apply_4(x_5, x_9, x_10, x_11, x_3);
            return x_12;
        }
        1 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_13 = lean_ctor_get(x_2, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_2, 1);
            lean_inc(x_14);
            lean_dec(x_2);
            x_15 = lean_apply_3(x_4, x_13, x_14, x_3);
            return x_15;
        }
        2 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            x_16 = lean_ctor_get(x_2, 0);
            lean_inc(x_16);
            x_17 = lean_ctor_get(x_2, 1);
            lean_inc(x_17);
            lean_dec(x_2);
            x_18 = lean_apply_3(x_6, x_16, x_17, x_3);
            return x_18;
        }
        3 => {
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_19 = lean_apply_1(x_7, x_3);
            return x_19;
        }
        4 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_20 = lean_apply_1(x_8, x_3);
            return x_20;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy_decEq___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy_decEq(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    x_4 = lean_box(x_3 as usize);
    return x_4;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecs___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecs(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_nop_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

unsafe fn _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1() -> LeanObjPtr {
    let mut x_1: LeanObjPtr = std::ptr::null_mut();
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_1 = lean_unsigned_to_nat(0);
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0;
    x_3 = lean_alloc_ctor(0, 2, 0);
    lean_ctor_set(x_3, 0, x_2);
    lean_ctor_set(x_3, 1, x_1);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_3 = lean_ctor_get(x_1, 0);
    lean_inc(x_3);
    x_4 = lean_ctor_get(x_1, 1);
    lean_inc(x_4);
    lean_dec(x_1);
    x_5 = lean_apply_2(x_2, x_3, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__5_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr, mut x_9: LeanObjPtr) -> LeanObjPtr {
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_2) {
        0 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            x_10 = lean_ctor_get(x_2, 0);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_2, 1);
            lean_inc(x_11);
            x_12 = lean_ctor_get(x_2, 2);
            lean_inc(x_12);
            lean_dec(x_2);
            x_13 = lean_apply_5(x_5, x_10, x_11, x_12, x_3, x_4);
            return x_13;
        }
        1 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            x_14 = lean_ctor_get(x_2, 0);
            lean_inc(x_14);
            x_15 = lean_ctor_get(x_2, 1);
            lean_inc(x_15);
            lean_dec(x_2);
            x_16 = lean_apply_4(x_6, x_14, x_15, x_3, x_4);
            return x_16;
        }
        2 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            x_17 = lean_ctor_get(x_2, 0);
            lean_inc(x_17);
            x_18 = lean_ctor_get(x_2, 1);
            lean_inc(x_18);
            lean_dec(x_2);
            x_19 = lean_apply_4(x_7, x_17, x_18, x_3, x_4);
            return x_19;
        }
        3 => {
            lean_dec(x_9);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_20 = lean_apply_2(x_8, x_3, x_4);
            return x_20;
        }
        4 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_21 = lean_apply_2(x_9, x_3, x_4);
            return x_21;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_decRef(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    x_3 = (!lean_is_exclusive(x_1)) as u8;
    if x_3 == 0
    {
        x_4 = lean_ctor_get(x_1, 0);
        x_5 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_decRef___lam__0 as *mut _, 3, 2);
        lean_closure_set(x_5, 0, x_2);
        lean_closure_set(x_5, 1, x_4);
        lean_ctor_set(x_1, 0, x_5);
        return x_1;
    }
    else
    {
        x_6 = lean_ctor_get(x_1, 0);
        x_7 = lean_ctor_get(x_1, 1);
        lean_inc(x_7);
        lean_inc(x_6);
        lean_dec(x_1);
        x_8 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_decRef___lam__0 as *mut _, 3, 2);
        lean_closure_set(x_8, 0, x_2);
        lean_closure_set(x_8, 1, x_6);
        x_9 = lean_alloc_ctor(0, 2, 0);
        lean_ctor_set(x_9, 0, x_8);
        lean_ctor_set(x_9, 1, x_7);
        return x_9;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc___lam__0(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: u8 = 0;
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_4 = lean_nat_dec_eq(x_3, x_1);
    if x_4 == 0
    {
        x_5 = lean_apply_1(x_2, x_3);
        return x_5;
    }
    else
    {
        lean_dec(x_3);
        lean_dec(x_2);
        x_6 = lean_unsigned_to_nat(1);
        return x_6;
    }
}

unsafe fn _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty() -> LeanObjPtr {
    let mut x_1: LeanObjPtr = std::ptr::null_mut();
    x_1 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1;
    return x_1;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___lam__0___boxed(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___lam__0(x_1);
    lean_dec(x_1);
    return x_2;
}

unsafe fn _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0() -> LeanObjPtr {
    let mut x_1: LeanObjPtr = std::ptr::null_mut();
    x_1 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___lam__0___boxed as *mut _, 1, 0);
    return x_1;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc___lam__0___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    x_4 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc___lam__0(x_1, x_2, x_3);
    lean_dec(x_1);
    return x_4;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_execute___lam__0___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_execute___lam__0(x_1, x_2, x_3, x_4);
    lean_dec(x_1);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_4 = lean_ctor_get(x_2, 0);
    lean_inc(x_4);
    x_5 = lean_ctor_get(x_2, 1);
    lean_inc(x_5);
    lean_dec(x_2);
    x_6 = lean_apply_2(x_3, x_4, x_5);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_execute(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    let mut x_22: LeanObjPtr = std::ptr::null_mut();
    let mut x_23: LeanObjPtr = std::ptr::null_mut();
    let mut x_24: LeanObjPtr = std::ptr::null_mut();
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                x_4 = lean_ctor_get(x_1, 0);
                lean_inc(x_4);
                x_5 = lean_ctor_get(x_1, 2);
                lean_inc(x_5);
                lean_dec(x_1);
                x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc(x_2);
                x_7 = lean_ctor_get(x_6, 0);
                lean_inc(x_7);
                x_8 = lean_ctor_get(x_6, 1);
                lean_inc(x_8);
                lean_dec(x_6);
                x_9 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_execute___lam__0___boxed as *mut _, 4, 3);
                lean_closure_set(x_9, 0, x_4);
                lean_closure_set(x_9, 1, x_3);
                lean_closure_set(x_9, 2, x_8);
                {
                    let mut _tmp_0: LeanObjPtr = x_5;
                    let mut _tmp_1: LeanObjPtr = x_7;
                    let mut _tmp_2: LeanObjPtr = x_9;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                    x_3 = _tmp_2;
                }
                continue '_start;
            }
            1 => {
                x_11 = lean_ctor_get(x_1, 0);
                lean_inc(x_11);
                x_12 = lean_ctor_get(x_1, 1);
                lean_inc(x_12);
                lean_dec(x_1);
                lean_inc(x_3);
                x_13 = lean_apply_1(x_3, x_11);
                if lean_obj_tag(x_13) == 0
                {
                    {
                        let mut _tmp_0: LeanObjPtr = x_12;
                        let mut _tmp_1: LeanObjPtr = x_2;
                        let mut _tmp_2: LeanObjPtr = x_3;
                        x_1 = _tmp_0;
                        x_2 = _tmp_1;
                        x_3 = _tmp_2;
                    }
                    continue '_start;
                }
                else
                {
                    x_15 = lean_ctor_get(x_13, 0);
                    lean_inc(x_15);
                    lean_dec(x_13);
                    x_16 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_incRef(x_2, x_15);
                    {
                        let mut _tmp_0: LeanObjPtr = x_12;
                        let mut _tmp_1: LeanObjPtr = x_16;
                        let mut _tmp_2: LeanObjPtr = x_3;
                        x_1 = _tmp_0;
                        x_2 = _tmp_1;
                        x_3 = _tmp_2;
                    }
                    continue '_start;
                }
            }
            2 => {
                x_18 = lean_ctor_get(x_1, 0);
                lean_inc(x_18);
                x_19 = lean_ctor_get(x_1, 1);
                lean_inc(x_19);
                lean_dec(x_1);
                lean_inc(x_3);
                x_20 = lean_apply_1(x_3, x_18);
                if lean_obj_tag(x_20) == 0
                {
                    {
                        let mut _tmp_0: LeanObjPtr = x_19;
                        let mut _tmp_1: LeanObjPtr = x_2;
                        let mut _tmp_2: LeanObjPtr = x_3;
                        x_1 = _tmp_0;
                        x_2 = _tmp_1;
                        x_3 = _tmp_2;
                    }
                    continue '_start;
                }
                else
                {
                    x_22 = lean_ctor_get(x_20, 0);
                    lean_inc(x_22);
                    lean_dec(x_20);
                    x_23 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_decRef(x_2, x_22);
                    {
                        let mut _tmp_0: LeanObjPtr = x_19;
                        let mut _tmp_1: LeanObjPtr = x_23;
                        let mut _tmp_2: LeanObjPtr = x_3;
                        x_1 = _tmp_0;
                        x_2 = _tmp_1;
                        x_3 = _tmp_2;
                    }
                    continue '_start;
                }
            }
            _ => {
                lean_dec(x_3);
                lean_dec(x_1);
                return x_2;
            }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_decRef___lam__0(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: u8 = 0;
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    x_4 = lean_nat_dec_eq(x_3, x_1);
    if x_4 == 0
    {
        lean_dec(x_1);
        x_5 = lean_apply_1(x_2, x_3);
        return x_5;
    }
    else
    {
        lean_dec(x_3);
        x_6 = lean_apply_1(x_2, x_1);
        x_7 = lean_unsigned_to_nat(1);
        x_8 = lean_nat_sub(x_6, x_7);
        lean_dec(x_6);
        return x_8;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_incRef___lam__0(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: u8 = 0;
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    x_4 = lean_nat_dec_eq(x_3, x_1);
    if x_4 == 0
    {
        lean_dec(x_1);
        x_5 = lean_apply_1(x_2, x_3);
        return x_5;
    }
    else
    {
        lean_dec(x_3);
        x_6 = lean_apply_1(x_2, x_1);
        x_7 = lean_unsigned_to_nat(1);
        x_8 = lean_nat_add(x_6, x_7);
        lean_dec(x_6);
        return x_8;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__3_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    if lean_obj_tag(x_2) == 0
    {
        lean_dec(x_3);
        x_5 = lean_box(0usize);
        x_6 = lean_apply_1(x_4, x_5);
        return x_6;
    }
    else
    {
        lean_dec(x_4);
        x_7 = lean_ctor_get(x_2, 0);
        lean_inc(x_7);
        lean_dec(x_2);
        x_8 = lean_apply_1(x_3, x_7);
        return x_8;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___lam__0(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    x_2 = lean_unsigned_to_nat(0);
    return x_2;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_incRef(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    x_3 = (!lean_is_exclusive(x_1)) as u8;
    if x_3 == 0
    {
        x_4 = lean_ctor_get(x_1, 0);
        x_5 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_incRef___lam__0 as *mut _, 3, 2);
        lean_closure_set(x_5, 0, x_2);
        lean_closure_set(x_5, 1, x_4);
        lean_ctor_set(x_1, 0, x_5);
        return x_1;
    }
    else
    {
        x_6 = lean_ctor_get(x_1, 0);
        x_7 = lean_ctor_get(x_1, 1);
        lean_inc(x_7);
        lean_inc(x_6);
        lean_dec(x_1);
        x_8 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_incRef___lam__0 as *mut _, 3, 2);
        lean_closure_set(x_8, 0, x_2);
        lean_closure_set(x_8, 1, x_6);
        x_9 = lean_alloc_ctor(0, 2, 0);
        lean_ctor_set(x_9, 0, x_8);
        lean_ctor_set(x_9, 1, x_7);
        return x_9;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__5_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_9 = lean_ctor_get(x_1, 0);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 1);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_1, 2);
            lean_inc(x_11);
            lean_dec(x_1);
            x_12 = lean_apply_5(x_4, x_9, x_10, x_11, x_2, x_3);
            return x_12;
        }
        1 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            x_13 = lean_ctor_get(x_1, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_1, 1);
            lean_inc(x_14);
            lean_dec(x_1);
            x_15 = lean_apply_4(x_5, x_13, x_14, x_2, x_3);
            return x_15;
        }
        2 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            x_16 = lean_ctor_get(x_1, 0);
            lean_inc(x_16);
            x_17 = lean_ctor_get(x_1, 1);
            lean_inc(x_17);
            lean_dec(x_1);
            x_18 = lean_apply_4(x_6, x_16, x_17, x_2, x_3);
            return x_18;
        }
        3 => {
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_19 = lean_apply_2(x_7, x_2, x_3);
            return x_19;
        }
        4 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_20 = lean_apply_2(x_8, x_2, x_3);
            return x_20;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_execute___lam__0(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: u8 = 0;
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    x_5 = lean_nat_dec_eq(x_4, x_1);
    if x_5 == 0
    {
        lean_dec(x_3);
        x_6 = lean_apply_1(x_2, x_4);
        return x_6;
    }
    else
    {
        lean_dec(x_4);
        lean_dec(x_2);
        x_7 = lean_alloc_ctor(1, 1, 0);
        lean_ctor_set(x_7, 0, x_3);
        return x_7;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: u8 = 0;
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    x_2 = (!lean_is_exclusive(x_1)) as u8;
    if x_2 == 0
    {
        x_3 = lean_ctor_get(x_1, 0);
        x_4 = lean_ctor_get(x_1, 1);
        lean_inc(x_4);
        x_5 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc___lam__0___boxed as *mut _, 3, 2);
        lean_closure_set(x_5, 0, x_4);
        lean_closure_set(x_5, 1, x_3);
        x_6 = lean_unsigned_to_nat(1);
        x_7 = lean_nat_add(x_4, x_6);
        lean_ctor_set(x_1, 1, x_7);
        lean_ctor_set(x_1, 0, x_5);
        x_8 = lean_alloc_ctor(0, 2, 0);
        lean_ctor_set(x_8, 0, x_1);
        lean_ctor_set(x_8, 1, x_4);
        return x_8;
    }
    else
    {
        x_9 = lean_ctor_get(x_1, 0);
        x_10 = lean_ctor_get(x_1, 1);
        lean_inc(x_10);
        lean_inc(x_9);
        lean_dec(x_1);
        lean_inc(x_10);
        x_11 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_alloc___lam__0___boxed as *mut _, 3, 2);
        lean_closure_set(x_11, 0, x_10);
        lean_closure_set(x_11, 1, x_9);
        x_12 = lean_unsigned_to_nat(1);
        x_13 = lean_nat_add(x_10, x_12);
        x_14 = lean_alloc_ctor(0, 2, 0);
        lean_ctor_set(x_14, 0, x_11);
        lean_ctor_set(x_14, 1, x_13);
        x_15 = lean_alloc_ctor(0, 2, 0);
        lean_ctor_set(x_15, 0, x_14);
        lean_ctor_set(x_15, 1, x_10);
        return x_15;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_Heap_0__AlmidePerceusBelt_execute_match__3_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr) -> LeanObjPtr {
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    if lean_obj_tag(x_1) == 0
    {
        lean_dec(x_2);
        x_4 = lean_box(0usize);
        x_5 = lean_apply_1(x_3, x_4);
        return x_5;
    }
    else
    {
        lean_dec(x_3);
        x_6 = lean_ctor_get(x_1, 0);
        lean_inc(x_6);
        lean_dec(x_1);
        x_7 = lean_apply_1(x_2, x_6);
        return x_7;
    }
}

static mut _G_initialized_0: bool = false;
pub unsafe fn initialize_AlmidePerceusBelt_FnBody(mut builtin: u8) -> LeanObjPtr {
    let mut res: LeanObjPtr = std::ptr::null_mut();
    if _G_initialized_0 { return lean_io_result_mk_ok(lean_box(0usize)); }
    _G_initialized_0 = true;
    res = initialize_Init(builtin);
    if lean_io_result_is_error(res) { return res; }
    lean_dec_ref(res);
    lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0 = _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0();
    lean_mark_persistent(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0);
    lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy = _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy();
    lean_mark_persistent(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy);
    return lean_io_result_mk_ok(lean_box(0usize));
}

static mut _G_initialized_1: bool = false;
pub unsafe fn initialize_AlmidePerceusBelt_Heap(mut builtin: u8) -> LeanObjPtr {
    let mut res: LeanObjPtr = std::ptr::null_mut();
    if _G_initialized_1 { return lean_io_result_mk_ok(lean_box(0usize)); }
    _G_initialized_1 = true;
    res = initialize_Init(builtin);
    if lean_io_result_is_error(res) { return res; }
    lean_dec_ref(res);
    res = initialize_AlmidePerceusBelt_FnBody(builtin);
    if lean_io_result_is_error(res) { return res; }
    lean_dec_ref(res);
    lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0 = _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0();
    lean_mark_persistent(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0);
    lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1 = _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1();
    lean_mark_persistent(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1);
    lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty = _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty();
    lean_mark_persistent(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty);
    return lean_io_result_mk_ok(lean_box(0usize));
}

