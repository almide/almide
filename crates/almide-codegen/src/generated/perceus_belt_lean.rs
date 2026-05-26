#![allow(non_snake_case, unused_mut, unused_variables, unused_assignments, non_upper_case_globals, unreachable_code)]
use lean_runtime::*;

static mut lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0: LeanObjPtr = std::ptr::null_mut();
static mut lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy: LeanObjPtr = std::ptr::null_mut();
static mut lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__0: LeanObjPtr = std::ptr::null_mut();
static mut lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty: LeanObjPtr = std::ptr::null_mut();
static mut lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Heap_empty___closed__1: LeanObjPtr = std::ptr::null_mut();

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_list_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_1, x_2);
    return x_3;
}

unsafe fn _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy() -> LeanObjPtr {
    let mut x_1: LeanObjPtr = std::ptr::null_mut();
    x_1 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0;
    return x_1;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: u8 = 0;
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: u8 = 0;
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: u8 = 0;
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: u8 = 0;
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    let mut x_22: LeanObjPtr = std::ptr::null_mut();
    let mut x_23: LeanObjPtr = std::ptr::null_mut();
    let mut x_24: LeanObjPtr = std::ptr::null_mut();
    let mut x_25: LeanObjPtr = std::ptr::null_mut();
    let mut x_26: u8 = 0;
    let mut x_27: LeanObjPtr = std::ptr::null_mut();
    let mut x_28: LeanObjPtr = std::ptr::null_mut();
    let mut x_29: LeanObjPtr = std::ptr::null_mut();
    let mut x_30: LeanObjPtr = std::ptr::null_mut();
    let mut x_31: LeanObjPtr = std::ptr::null_mut();
    let mut x_32: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_2 = (!lean_is_exclusive(x_1)) as u8;
            if x_2 == 0
            {
                x_3 = lean_ctor_get(x_1, 0);
                x_4 = lean_ctor_get(x_1, 1);
                x_5 = lean_ctor_get(x_1, 2);
                x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_isHeap(x_4);
                if x_6 == 0
                {
                    x_7 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_5);
                    lean_ctor_set(x_1, 2, x_7);
                    return x_1;
                }
                else
                {
                    x_8 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_5);
                    lean_inc(x_3);
                    x_9 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_8, x_3);
                    lean_ctor_set(x_1, 2, x_9);
                    return x_1;
                }
            }
            else
            {
                x_10 = lean_ctor_get(x_1, 0);
                x_11 = lean_ctor_get(x_1, 1);
                x_12 = lean_ctor_get(x_1, 2);
                lean_inc(x_12);
                lean_inc(x_11);
                lean_inc(x_10);
                lean_dec(x_1);
                x_13 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_isHeap(x_11);
                if x_13 == 0
                {
                    x_14 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_12);
                    x_15 = lean_alloc_ctor(0, 3, 0);
                    lean_ctor_set(x_15, 0, x_10);
                    lean_ctor_set(x_15, 1, x_11);
                    lean_ctor_set(x_15, 2, x_14);
                    return x_15;
                }
                else
                {
                    x_16 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_12);
                    lean_inc(x_10);
                    x_17 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_16, x_10);
                    x_18 = lean_alloc_ctor(0, 3, 0);
                    lean_ctor_set(x_18, 0, x_10);
                    lean_ctor_set(x_18, 1, x_11);
                    lean_ctor_set(x_18, 2, x_17);
                    return x_18;
                }
            }
        }
        1 => {
            x_19 = (!lean_is_exclusive(x_1)) as u8;
            if x_19 == 0
            {
                x_20 = lean_ctor_get(x_1, 1);
                x_21 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_20);
                lean_ctor_set(x_1, 1, x_21);
                return x_1;
            }
            else
            {
                x_22 = lean_ctor_get(x_1, 0);
                x_23 = lean_ctor_get(x_1, 1);
                lean_inc(x_23);
                lean_inc(x_22);
                lean_dec(x_1);
                x_24 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_23);
                x_25 = lean_alloc_ctor(1, 2, 0);
                lean_ctor_set(x_25, 0, x_22);
                lean_ctor_set(x_25, 1, x_24);
                return x_25;
            }
        }
        2 => {
            x_26 = (!lean_is_exclusive(x_1)) as u8;
            if x_26 == 0
            {
                x_27 = lean_ctor_get(x_1, 1);
                x_28 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_27);
                lean_ctor_set(x_1, 1, x_28);
                return x_1;
            }
            else
            {
                x_29 = lean_ctor_get(x_1, 0);
                x_30 = lean_ctor_get(x_1, 1);
                lean_inc(x_30);
                lean_inc(x_29);
                lean_dec(x_1);
                x_31 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_perceusTransform(x_30);
                x_32 = lean_alloc_ctor(2, 2, 0);
                lean_ctor_set(x_32, 0, x_29);
                lean_ctor_set(x_32, 1, x_31);
                return x_32;
            }
        }
        _ => {
            return x_1;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ret_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_string_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ite_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_insertDecBeforeEndCF_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_8 = lean_ctor_get(x_1, 0);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_1, 1);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 2);
            lean_inc(x_10);
            lean_dec(x_1);
            x_11 = lean_apply_3(x_2, x_8, x_9, x_10);
            return x_11;
        }
        1 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_2);
            x_12 = lean_ctor_get(x_1, 0);
            lean_inc(x_12);
            x_13 = lean_ctor_get(x_1, 1);
            lean_inc(x_13);
            lean_dec(x_1);
            x_14 = lean_apply_2(x_3, x_12, x_13);
            return x_14;
        }
        2 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            lean_dec(x_2);
            x_15 = lean_ctor_get(x_1, 0);
            lean_inc(x_15);
            x_16 = lean_ctor_get(x_1, 1);
            lean_inc(x_16);
            lean_dec(x_1);
            x_17 = lean_apply_2(x_4, x_15, x_16);
            return x_17;
        }
        3 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            lean_dec(x_2);
            x_18 = lean_ctor_get(x_1, 0);
            lean_inc(x_18);
            x_19 = lean_ctor_get(x_1, 1);
            lean_inc(x_19);
            lean_dec(x_1);
            x_20 = lean_apply_2(x_5, x_18, x_19);
            return x_20;
        }
        4 => {
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            lean_dec(x_2);
            x_21 = lean_box(0usize);
            x_22 = lean_apply_1(x_6, x_21);
            return x_22;
        }
        5 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            lean_dec(x_2);
            x_23 = lean_box(0usize);
            x_24 = lean_apply_1(x_7, x_23);
            return x_24;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_vdecl_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_dec_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countIncs_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_8 = lean_ctor_get(x_1, 0);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_1, 1);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 2);
            lean_inc(x_10);
            lean_dec(x_1);
            x_11 = lean_apply_4(x_4, x_8, x_9, x_10, x_2);
            return x_11;
        }
        1 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_12 = lean_ctor_get(x_1, 0);
            lean_inc(x_12);
            x_13 = lean_ctor_get(x_1, 1);
            lean_inc(x_13);
            lean_dec(x_1);
            x_14 = lean_apply_3(x_3, x_12, x_13, x_2);
            return x_14;
        }
        2 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_15 = lean_ctor_get(x_1, 0);
            lean_inc(x_15);
            x_16 = lean_ctor_get(x_1, 1);
            lean_inc(x_16);
            lean_dec(x_1);
            x_17 = lean_apply_3(x_5, x_15, x_16, x_2);
            return x_17;
        }
        3 => {
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_18 = lean_apply_1(x_6, x_2);
            return x_18;
        }
        4 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_19 = lean_apply_1(x_7, x_2);
            return x_19;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_inc_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: u8 = 0;
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: u8 = 0;
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    let mut x_22: LeanObjPtr = std::ptr::null_mut();
    let mut x_23: LeanObjPtr = std::ptr::null_mut();
    let mut x_24: LeanObjPtr = std::ptr::null_mut();
    let mut x_25: u8 = 0;
    let mut x_26: LeanObjPtr = std::ptr::null_mut();
    let mut x_27: LeanObjPtr = std::ptr::null_mut();
    let mut x_28: LeanObjPtr = std::ptr::null_mut();
    let mut x_29: LeanObjPtr = std::ptr::null_mut();
    let mut x_30: LeanObjPtr = std::ptr::null_mut();
    let mut x_31: LeanObjPtr = std::ptr::null_mut();
    let mut x_32: LeanObjPtr = std::ptr::null_mut();
    let mut x_33: LeanObjPtr = std::ptr::null_mut();
    let mut x_34: LeanObjPtr = std::ptr::null_mut();
    let mut x_35: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_3 = (!lean_is_exclusive(x_1)) as u8;
            if x_3 == 0
            {
                x_4 = lean_ctor_get(x_1, 2);
                x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_4, x_2);
                lean_ctor_set(x_1, 2, x_5);
                return x_1;
            }
            else
            {
                x_6 = lean_ctor_get(x_1, 0);
                x_7 = lean_ctor_get(x_1, 1);
                x_8 = lean_ctor_get(x_1, 2);
                lean_inc(x_8);
                lean_inc(x_7);
                lean_inc(x_6);
                lean_dec(x_1);
                x_9 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_8, x_2);
                x_10 = lean_alloc_ctor(0, 3, 0);
                lean_ctor_set(x_10, 0, x_6);
                lean_ctor_set(x_10, 1, x_7);
                lean_ctor_set(x_10, 2, x_9);
                return x_10;
            }
        }
        1 => {
            x_11 = (!lean_is_exclusive(x_1)) as u8;
            if x_11 == 0
            {
                x_12 = lean_ctor_get(x_1, 1);
                x_13 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_12, x_2);
                lean_ctor_set(x_1, 1, x_13);
                return x_1;
            }
            else
            {
                x_14 = lean_ctor_get(x_1, 0);
                x_15 = lean_ctor_get(x_1, 1);
                lean_inc(x_15);
                lean_inc(x_14);
                lean_dec(x_1);
                x_16 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_15, x_2);
                x_17 = lean_alloc_ctor(1, 2, 0);
                lean_ctor_set(x_17, 0, x_14);
                lean_ctor_set(x_17, 1, x_16);
                return x_17;
            }
        }
        2 => {
            x_18 = (!lean_is_exclusive(x_1)) as u8;
            if x_18 == 0
            {
                x_19 = lean_ctor_get(x_1, 1);
                x_20 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_19, x_2);
                lean_ctor_set(x_1, 1, x_20);
                return x_1;
            }
            else
            {
                x_21 = lean_ctor_get(x_1, 0);
                x_22 = lean_ctor_get(x_1, 1);
                lean_inc(x_22);
                lean_inc(x_21);
                lean_dec(x_1);
                x_23 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_22, x_2);
                x_24 = lean_alloc_ctor(2, 2, 0);
                lean_ctor_set(x_24, 0, x_21);
                lean_ctor_set(x_24, 1, x_23);
                return x_24;
            }
        }
        3 => {
            x_25 = (!lean_is_exclusive(x_1)) as u8;
            if x_25 == 0
            {
                x_26 = lean_ctor_get(x_1, 0);
                x_27 = lean_ctor_get(x_1, 1);
                lean_inc(x_2);
                x_28 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_26, x_2);
                x_29 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_27, x_2);
                lean_ctor_set(x_1, 1, x_29);
                lean_ctor_set(x_1, 0, x_28);
                return x_1;
            }
            else
            {
                x_30 = lean_ctor_get(x_1, 0);
                x_31 = lean_ctor_get(x_1, 1);
                lean_inc(x_31);
                lean_inc(x_30);
                lean_dec(x_1);
                lean_inc(x_2);
                x_32 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_30, x_2);
                x_33 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEndCF(x_31, x_2);
                x_34 = lean_alloc_ctor(3, 2, 0);
                lean_ctor_set(x_34, 0, x_32);
                lean_ctor_set(x_34, 1, x_33);
                return x_34;
            }
        }
        _ => {
            x_35 = lean_alloc_ctor(2, 2, 0);
            lean_ctor_set(x_35, 0, x_2);
            lean_ctor_set(x_35, 1, x_1);
            return x_35;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorIdx___boxed(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorIdx(x_1);
    lean_dec(x_1);
    return x_2;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            x_3 = lean_ctor_get(x_1, 0);
            lean_inc(x_3);
            x_4 = lean_ctor_get(x_1, 1);
            lean_inc(x_4);
            x_5 = lean_ctor_get(x_1, 2);
            lean_inc(x_5);
            lean_dec(x_1);
            x_6 = lean_apply_3(x_2, x_3, x_4, x_5);
            return x_6;
        }
        1 => {
            x_7 = lean_ctor_get(x_1, 0);
            lean_inc(x_7);
            x_8 = lean_ctor_get(x_1, 1);
            lean_inc(x_8);
            lean_dec(x_1);
            x_9 = lean_apply_2(x_2, x_7, x_8);
            return x_9;
        }
        2 => {
            x_10 = lean_ctor_get(x_1, 0);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_1, 1);
            lean_inc(x_11);
            lean_dec(x_1);
            x_12 = lean_apply_2(x_2, x_10, x_11);
            return x_12;
        }
        _ => {
            lean_dec(x_1);
            return x_2;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_unit_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_int_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: u8 = 0;
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: u8 = 0;
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: LeanObjPtr = std::ptr::null_mut();
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    let mut x_22: LeanObjPtr = std::ptr::null_mut();
    let mut x_23: LeanObjPtr = std::ptr::null_mut();
    let mut x_24: LeanObjPtr = std::ptr::null_mut();
    let mut x_25: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_3 = (!lean_is_exclusive(x_1)) as u8;
            if x_3 == 0
            {
                x_4 = lean_ctor_get(x_1, 2);
                x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_4, x_2);
                lean_ctor_set(x_1, 2, x_5);
                return x_1;
            }
            else
            {
                x_6 = lean_ctor_get(x_1, 0);
                x_7 = lean_ctor_get(x_1, 1);
                x_8 = lean_ctor_get(x_1, 2);
                lean_inc(x_8);
                lean_inc(x_7);
                lean_inc(x_6);
                lean_dec(x_1);
                x_9 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_8, x_2);
                x_10 = lean_alloc_ctor(0, 3, 0);
                lean_ctor_set(x_10, 0, x_6);
                lean_ctor_set(x_10, 1, x_7);
                lean_ctor_set(x_10, 2, x_9);
                return x_10;
            }
        }
        1 => {
            x_11 = (!lean_is_exclusive(x_1)) as u8;
            if x_11 == 0
            {
                x_12 = lean_ctor_get(x_1, 1);
                x_13 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_12, x_2);
                lean_ctor_set(x_1, 1, x_13);
                return x_1;
            }
            else
            {
                x_14 = lean_ctor_get(x_1, 0);
                x_15 = lean_ctor_get(x_1, 1);
                lean_inc(x_15);
                lean_inc(x_14);
                lean_dec(x_1);
                x_16 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_15, x_2);
                x_17 = lean_alloc_ctor(1, 2, 0);
                lean_ctor_set(x_17, 0, x_14);
                lean_ctor_set(x_17, 1, x_16);
                return x_17;
            }
        }
        2 => {
            x_18 = (!lean_is_exclusive(x_1)) as u8;
            if x_18 == 0
            {
                x_19 = lean_ctor_get(x_1, 1);
                x_20 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_19, x_2);
                lean_ctor_set(x_1, 1, x_20);
                return x_1;
            }
            else
            {
                x_21 = lean_ctor_get(x_1, 0);
                x_22 = lean_ctor_get(x_1, 1);
                lean_inc(x_22);
                lean_inc(x_21);
                lean_dec(x_1);
                x_23 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_insertDecBeforeEnd(x_22, x_2);
                x_24 = lean_alloc_ctor(2, 2, 0);
                lean_ctor_set(x_24, 0, x_21);
                lean_ctor_set(x_24, 1, x_23);
                return x_24;
            }
        }
        _ => {
            x_25 = lean_alloc_ctor(2, 2, 0);
            lean_ctor_set(x_25, 0, x_2);
            lean_ctor_set(x_25, 1, x_1);
            return x_25;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncs___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncs(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countDecs_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_8 = lean_ctor_get(x_1, 0);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_1, 1);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 2);
            lean_inc(x_10);
            lean_dec(x_1);
            x_11 = lean_apply_4(x_4, x_8, x_9, x_10, x_2);
            return x_11;
        }
        1 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_12 = lean_ctor_get(x_1, 0);
            lean_inc(x_12);
            x_13 = lean_ctor_get(x_1, 1);
            lean_inc(x_13);
            lean_dec(x_1);
            x_14 = lean_apply_3(x_5, x_12, x_13, x_2);
            return x_14;
        }
        2 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_15 = lean_ctor_get(x_1, 0);
            lean_inc(x_15);
            x_16 = lean_ctor_get(x_1, 1);
            lean_inc(x_16);
            lean_dec(x_1);
            x_17 = lean_apply_3(x_3, x_15, x_16, x_2);
            return x_17;
        }
        3 => {
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_18 = lean_apply_1(x_6, x_2);
            return x_18;
        }
        4 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_19 = lean_apply_1(x_7, x_2);
            return x_19;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_isHeap(mut x_1: LeanObjPtr) -> u8 {
    let mut x_2: u8 = 0;
    let mut x_3: u8 = 0;
    let mut x_4: u8 = 0;
    match lean_obj_tag(x_1) {
        1 => {
            x_2 = 1;
            return x_2;
        }
        2 => {
            x_3 = 1;
            return x_3;
        }
        _ => {
            x_4 = 0;
            return x_4;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecs(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut jp12_x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: u8 = 0;
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                x_3 = lean_ctor_get(x_1, 2);
                {
                    let mut _tmp_0: LeanObjPtr = x_3;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            1 => {
                x_5 = lean_ctor_get(x_1, 1);
                {
                    let mut _tmp_0: LeanObjPtr = x_5;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            2 => {
                x_7 = lean_ctor_get(x_1, 0);
                x_8 = lean_ctor_get(x_1, 1);
                'block_j12: loop {
                    x_13 = lean_nat_dec_eq(x_7, x_2);
                    if x_13 == 0
                    {
                        x_14 = lean_unsigned_to_nat(0);
                        jp12_x_9 = x_14;
                        break 'block_j12;
                    }
                    else
                    {
                        x_15 = lean_unsigned_to_nat(1);
                        jp12_x_9 = x_15;
                        break 'block_j12;
                    }
                    break 'block_j12;
                }
                let x_9 = jp12_x_9;
                x_10 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecs(x_8, x_2);
                x_11 = lean_nat_add(x_9, x_10);
                lean_dec(x_10);
                return x_11;
            }
            _ => {
                x_16 = lean_unsigned_to_nat(0);
                return x_16;
            }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ret_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_string_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ite_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_3, x_5);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_inc_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_3, x_5);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy_beq(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> u8 {
    let mut x_3: u8 = 0;
    let mut x_4: u8 = 0;
    let mut x_5: u8 = 0;
    let mut x_6: u8 = 0;
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: u8 = 0;
    let mut x_10: u8 = 0;
    let mut x_11: u8 = 0;
    let mut x_12: u8 = 0;
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                if lean_obj_tag(x_2) == 0
                {
                    x_3 = 1;
                    return x_3;
                }
                else
                {
                    x_4 = 0;
                    return x_4;
                }
            }
            1 => {
                if lean_obj_tag(x_2) == 1
                {
                    x_5 = 1;
                    return x_5;
                }
                else
                {
                    x_6 = 0;
                    return x_6;
                }
            }
            2 => {
                if lean_obj_tag(x_2) == 2
                {
                    x_7 = lean_ctor_get(x_1, 0);
                    x_8 = lean_ctor_get(x_2, 0);
                    {
                        let mut _tmp_0: LeanObjPtr = x_7;
                        let mut _tmp_1: LeanObjPtr = x_8;
                        x_1 = _tmp_0;
                        x_2 = _tmp_1;
                    }
                    continue '_start;
                }
                else
                {
                    x_10 = 0;
                    return x_10;
                }
            }
            3 => {
                if lean_obj_tag(x_2) == 3
                {
                    x_11 = 1;
                    return x_11;
                }
                else
                {
                    x_12 = 0;
                    return x_12;
                }
            }
            _ => { unreachable!(); }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut jp12_x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: u8 = 0;
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: u8 = 0;
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                x_3 = lean_ctor_get(x_1, 2);
                {
                    let mut _tmp_0: LeanObjPtr = x_3;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            1 => {
                x_5 = lean_ctor_get(x_1, 1);
                {
                    let mut _tmp_0: LeanObjPtr = x_5;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            2 => {
                x_7 = lean_ctor_get(x_1, 0);
                x_8 = lean_ctor_get(x_1, 1);
                'block_j12: loop {
                    x_13 = lean_nat_dec_eq(x_7, x_2);
                    if x_13 == 0
                    {
                        x_14 = lean_unsigned_to_nat(0);
                        jp12_x_9 = x_14;
                        break 'block_j12;
                    }
                    else
                    {
                        x_15 = lean_unsigned_to_nat(1);
                        jp12_x_9 = x_15;
                        break 'block_j12;
                    }
                    break 'block_j12;
                }
                let x_9 = jp12_x_9;
                x_10 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF(x_8, x_2);
                x_11 = lean_nat_add(x_9, x_10);
                lean_dec(x_10);
                return x_11;
            }
            3 => {
                x_16 = lean_ctor_get(x_1, 0);
                x_17 = lean_ctor_get(x_1, 1);
                x_18 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF(x_16, x_2);
                x_19 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF(x_17, x_2);
                x_20 = lean_nat_dec_le(x_18, x_19);
                if x_20 == 0
                {
                    lean_dec(x_18);
                    return x_19;
                }
                else
                {
                    lean_dec(x_19);
                    return x_18;
                }
            }
            _ => {
                x_21 = lean_unsigned_to_nat(0);
                return x_21;
            }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim(x_2, x_3, x_5);
    lean_dec(x_2);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countDecs_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
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
            lean_dec(x_5);
            lean_dec(x_4);
            x_13 = lean_ctor_get(x_2, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_2, 1);
            lean_inc(x_14);
            lean_dec(x_2);
            x_15 = lean_apply_3(x_6, x_13, x_14, x_3);
            return x_15;
        }
        2 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_16 = lean_ctor_get(x_2, 0);
            lean_inc(x_16);
            x_17 = lean_ctor_get(x_2, 1);
            lean_inc(x_17);
            lean_dec(x_2);
            x_18 = lean_apply_3(x_4, x_16, x_17, x_3);
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

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_vdecl_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_vdecl_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_perceusTransform_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_2) {
        0 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_8 = lean_ctor_get(x_2, 0);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_2, 1);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_2, 2);
            lean_inc(x_10);
            lean_dec(x_2);
            x_11 = lean_apply_3(x_3, x_8, x_9, x_10);
            return x_11;
        }
        1 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_12 = lean_ctor_get(x_2, 0);
            lean_inc(x_12);
            x_13 = lean_ctor_get(x_2, 1);
            lean_inc(x_13);
            lean_dec(x_2);
            x_14 = lean_apply_2(x_4, x_12, x_13);
            return x_14;
        }
        2 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_15 = lean_ctor_get(x_2, 0);
            lean_inc(x_15);
            x_16 = lean_ctor_get(x_2, 1);
            lean_inc(x_16);
            lean_dec(x_2);
            x_17 = lean_apply_2(x_5, x_15, x_16);
            return x_17;
        }
        3 => {
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_18 = lean_box(0usize);
            x_19 = lean_apply_1(x_6, x_18);
            return x_19;
        }
        4 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_20 = lean_box(0usize);
            x_21 = lean_apply_1(x_7, x_20);
            return x_21;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_inc_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorIdx___boxed(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorIdx(x_1);
    lean_dec(x_1);
    return x_2;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    if lean_obj_tag(x_1) == 2
    {
        x_3 = lean_ctor_get(x_1, 0);
        lean_inc(x_3);
        lean_dec(x_1);
        x_4 = lean_apply_1(x_2, x_3);
        return x_4;
    }
    else
    {
        lean_dec(x_1);
        return x_2;
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_nop_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorIdx(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_2 = lean_unsigned_to_nat(0);
            return x_2;
        }
        1 => {
            x_3 = lean_unsigned_to_nat(1);
            return x_3;
        }
        2 => {
            x_4 = lean_unsigned_to_nat(2);
            return x_4;
        }
        3 => {
            x_5 = lean_unsigned_to_nat(3);
            return x_5;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorIdx(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_2 = lean_unsigned_to_nat(0);
            return x_2;
        }
        1 => {
            x_3 = lean_unsigned_to_nat(1);
            return x_3;
        }
        2 => {
            x_4 = lean_unsigned_to_nat(2);
            return x_4;
        }
        3 => {
            x_5 = lean_unsigned_to_nat(3);
            return x_5;
        }
        4 => {
            x_6 = lean_unsigned_to_nat(4);
            return x_6;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_vdecl_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_inc_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ret_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_isHeap___boxed(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: u8 = 0;
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_isHeap(x_1);
    lean_dec(x_1);
    x_3 = lean_box(x_2 as usize);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncs(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut jp10_x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: u8 = 0;
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                x_3 = lean_ctor_get(x_1, 2);
                {
                    let mut _tmp_0: LeanObjPtr = x_3;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            1 => {
                x_5 = lean_ctor_get(x_1, 0);
                x_6 = lean_ctor_get(x_1, 1);
                'block_j10: loop {
                    x_11 = lean_nat_dec_eq(x_5, x_2);
                    if x_11 == 0
                    {
                        x_12 = lean_unsigned_to_nat(0);
                        jp10_x_7 = x_12;
                        break 'block_j10;
                    }
                    else
                    {
                        x_13 = lean_unsigned_to_nat(1);
                        jp10_x_7 = x_13;
                        break 'block_j10;
                    }
                    break 'block_j10;
                }
                let x_7 = jp10_x_7;
                x_8 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncs(x_6, x_2);
                x_9 = lean_nat_add(x_7, x_8);
                lean_dec(x_8);
                return x_9;
            }
            2 => {
                x_14 = lean_ctor_get(x_1, 1);
                {
                    let mut _tmp_0: LeanObjPtr = x_14;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            _ => {
                x_16 = lean_unsigned_to_nat(0);
                return x_16;
            }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_perceusTransform_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_7 = lean_ctor_get(x_1, 0);
            lean_inc(x_7);
            x_8 = lean_ctor_get(x_1, 1);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_1, 2);
            lean_inc(x_9);
            lean_dec(x_1);
            x_10 = lean_apply_3(x_2, x_7, x_8, x_9);
            return x_10;
        }
        1 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_2);
            x_11 = lean_ctor_get(x_1, 0);
            lean_inc(x_11);
            x_12 = lean_ctor_get(x_1, 1);
            lean_inc(x_12);
            lean_dec(x_1);
            x_13 = lean_apply_2(x_3, x_11, x_12);
            return x_13;
        }
        2 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            lean_dec(x_2);
            x_14 = lean_ctor_get(x_1, 0);
            lean_inc(x_14);
            x_15 = lean_ctor_get(x_1, 1);
            lean_inc(x_15);
            lean_dec(x_1);
            x_16 = lean_apply_2(x_4, x_14, x_15);
            return x_16;
        }
        3 => {
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            lean_dec(x_2);
            x_17 = lean_box(0usize);
            x_18 = lean_apply_1(x_5, x_17);
            return x_18;
        }
        4 => {
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            lean_dec(x_2);
            x_19 = lean_box(0usize);
            x_20 = lean_apply_1(x_6, x_19);
            return x_20;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_insertDecBeforeEnd_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_8 = lean_ctor_get(x_1, 0);
            lean_inc(x_8);
            x_9 = lean_ctor_get(x_1, 1);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 2);
            lean_inc(x_10);
            lean_dec(x_1);
            x_11 = lean_apply_4(x_3, x_8, x_9, x_10, x_2);
            return x_11;
        }
        1 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_12 = lean_ctor_get(x_1, 0);
            lean_inc(x_12);
            x_13 = lean_ctor_get(x_1, 1);
            lean_inc(x_13);
            lean_dec(x_1);
            x_14 = lean_apply_3(x_4, x_12, x_13, x_2);
            return x_14;
        }
        2 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_15 = lean_ctor_get(x_1, 0);
            lean_inc(x_15);
            x_16 = lean_ctor_get(x_1, 1);
            lean_inc(x_16);
            lean_dec(x_1);
            x_17 = lean_apply_3(x_5, x_15, x_16, x_2);
            return x_17;
        }
        3 => {
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_18 = lean_apply_1(x_6, x_2);
            return x_18;
        }
        4 => {
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_19 = lean_apply_1(x_7, x_2);
            return x_19;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ret_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_dec_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countDecsCF(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_unit_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorIdx(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_1) {
        0 => {
            x_2 = lean_unsigned_to_nat(0);
            return x_2;
        }
        1 => {
            x_3 = lean_unsigned_to_nat(1);
            return x_3;
        }
        2 => {
            x_4 = lean_unsigned_to_nat(2);
            return x_4;
        }
        3 => {
            x_5 = lean_unsigned_to_nat(3);
            return x_5;
        }
        4 => {
            x_6 = lean_unsigned_to_nat(4);
            return x_6;
        }
        5 => {
            x_7 = lean_unsigned_to_nat(5);
            return x_7;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorIdx___boxed(mut x_1: LeanObjPtr) -> LeanObjPtr {
    let mut x_2: LeanObjPtr = std::ptr::null_mut();
    x_2 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorIdx(x_1);
    lean_dec(x_1);
    return x_2;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_dec_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_insertDecBeforeEndCF_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
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
    let mut x_25: LeanObjPtr = std::ptr::null_mut();
    match lean_obj_tag(x_2) {
        0 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_9 = lean_ctor_get(x_2, 0);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_2, 1);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_2, 2);
            lean_inc(x_11);
            lean_dec(x_2);
            x_12 = lean_apply_3(x_3, x_9, x_10, x_11);
            return x_12;
        }
        1 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_13 = lean_ctor_get(x_2, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_2, 1);
            lean_inc(x_14);
            lean_dec(x_2);
            x_15 = lean_apply_2(x_4, x_13, x_14);
            return x_15;
        }
        2 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_16 = lean_ctor_get(x_2, 0);
            lean_inc(x_16);
            x_17 = lean_ctor_get(x_2, 1);
            lean_inc(x_17);
            lean_dec(x_2);
            x_18 = lean_apply_2(x_5, x_16, x_17);
            return x_18;
        }
        3 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_19 = lean_ctor_get(x_2, 0);
            lean_inc(x_19);
            x_20 = lean_ctor_get(x_2, 1);
            lean_inc(x_20);
            lean_dec(x_2);
            x_21 = lean_apply_2(x_6, x_19, x_20);
            return x_21;
        }
        4 => {
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_22 = lean_box(0usize);
            x_23 = lean_apply_1(x_7, x_22);
            return x_23;
        }
        5 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_24 = lean_box(0usize);
            x_25 = lean_apply_1(x_8, x_24);
            return x_25;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            x_3 = lean_ctor_get(x_1, 0);
            lean_inc(x_3);
            x_4 = lean_ctor_get(x_1, 1);
            lean_inc(x_4);
            x_5 = lean_ctor_get(x_1, 2);
            lean_inc(x_5);
            lean_dec(x_1);
            x_6 = lean_apply_3(x_2, x_3, x_4, x_5);
            return x_6;
        }
        1 => {
            x_7 = lean_ctor_get(x_1, 0);
            lean_inc(x_7);
            x_8 = lean_ctor_get(x_1, 1);
            lean_inc(x_8);
            lean_dec(x_1);
            x_9 = lean_apply_2(x_2, x_7, x_8);
            return x_9;
        }
        2 => {
            x_10 = lean_ctor_get(x_1, 0);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_1, 1);
            lean_inc(x_11);
            lean_dec(x_1);
            x_12 = lean_apply_2(x_2, x_10, x_11);
            return x_12;
        }
        3 => {
            x_13 = lean_ctor_get(x_1, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_1, 1);
            lean_inc(x_14);
            lean_dec(x_1);
            x_15 = lean_apply_2(x_2, x_13, x_14);
            return x_15;
        }
        _ => {
            lean_dec(x_1);
            return x_2;
        }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_dec_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_nop_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_3, x_5);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> u8 {
    let mut x_3: u8 = 0;
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy_decEq(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_insertDecBeforeEnd_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
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
            lean_dec(x_5);
            x_9 = lean_ctor_get(x_2, 0);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_2, 1);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_2, 2);
            lean_inc(x_11);
            lean_dec(x_2);
            x_12 = lean_apply_4(x_4, x_9, x_10, x_11, x_3);
            return x_12;
        }
        1 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            x_13 = lean_ctor_get(x_2, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_2, 1);
            lean_inc(x_14);
            lean_dec(x_2);
            x_15 = lean_apply_3(x_5, x_13, x_14, x_3);
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

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy_decEq(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> u8 {
    let mut x_3: u8 = 0;
    let mut x_4: u8 = 0;
    let mut x_5: u8 = 0;
    let mut x_6: u8 = 0;
    let mut x_7: u8 = 0;
    let mut x_8: u8 = 0;
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_10: u8 = 0;
    let mut x_11: LeanObjPtr = std::ptr::null_mut();
    let mut x_12: u8 = 0;
    let mut x_13: u8 = 0;
    let mut x_14: u8 = 0;
    let mut x_15: u8 = 0;
    match lean_obj_tag(x_1) {
        0 => {
            match lean_obj_tag(x_2) {
                0 => {
                    x_3 = 1;
                    return x_3;
                }
                2 => {
                    x_4 = 0;
                    return x_4;
                }
                _ => {
                    x_5 = 0;
                    return x_5;
                }
            }
        }
        1 => {
            match lean_obj_tag(x_2) {
                1 => {
                    x_6 = 1;
                    return x_6;
                }
                2 => {
                    x_7 = 0;
                    return x_7;
                }
                _ => {
                    x_8 = 0;
                    return x_8;
                }
            }
        }
        2 => {
            x_9 = lean_ctor_get(x_1, 0);
            x_10 = 0;
            if lean_obj_tag(x_2) == 2
            {
                x_11 = lean_ctor_get(x_2, 0);
                x_12 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy_decEq(x_9, x_11);
                if x_12 == 0
                {
                    return x_10;
                }
                else
                {
                    return x_12;
                }
            }
            else
            {
                return x_10;
            }
        }
        3 => {
            match lean_obj_tag(x_2) {
                2 => {
                    x_13 = 0;
                    return x_13;
                }
                3 => {
                    x_14 = 1;
                    return x_14;
                }
                _ => {
                    x_15 = 0;
                    return x_15;
                }
            }
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut jp10_x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    let mut x_7: LeanObjPtr = std::ptr::null_mut();
    let mut x_8: LeanObjPtr = std::ptr::null_mut();
    let mut x_9: LeanObjPtr = std::ptr::null_mut();
    let mut x_11: u8 = 0;
    let mut x_12: LeanObjPtr = std::ptr::null_mut();
    let mut x_13: LeanObjPtr = std::ptr::null_mut();
    let mut x_14: LeanObjPtr = std::ptr::null_mut();
    let mut x_15: LeanObjPtr = std::ptr::null_mut();
    let mut x_16: LeanObjPtr = std::ptr::null_mut();
    let mut x_17: LeanObjPtr = std::ptr::null_mut();
    let mut x_18: LeanObjPtr = std::ptr::null_mut();
    let mut x_19: LeanObjPtr = std::ptr::null_mut();
    let mut x_20: u8 = 0;
    let mut x_21: LeanObjPtr = std::ptr::null_mut();
    '_start: loop {
        match lean_obj_tag(x_1) {
            0 => {
                x_3 = lean_ctor_get(x_1, 2);
                {
                    let mut _tmp_0: LeanObjPtr = x_3;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            1 => {
                x_5 = lean_ctor_get(x_1, 0);
                x_6 = lean_ctor_get(x_1, 1);
                'block_j10: loop {
                    x_11 = lean_nat_dec_eq(x_5, x_2);
                    if x_11 == 0
                    {
                        x_12 = lean_unsigned_to_nat(0);
                        jp10_x_7 = x_12;
                        break 'block_j10;
                    }
                    else
                    {
                        x_13 = lean_unsigned_to_nat(1);
                        jp10_x_7 = x_13;
                        break 'block_j10;
                    }
                    break 'block_j10;
                }
                let x_7 = jp10_x_7;
                x_8 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF(x_6, x_2);
                x_9 = lean_nat_add(x_7, x_8);
                lean_dec(x_8);
                return x_9;
            }
            2 => {
                x_14 = lean_ctor_get(x_1, 1);
                {
                    let mut _tmp_0: LeanObjPtr = x_14;
                    let mut _tmp_1: LeanObjPtr = x_2;
                    x_1 = _tmp_0;
                    x_2 = _tmp_1;
                }
                continue '_start;
            }
            3 => {
                x_16 = lean_ctor_get(x_1, 0);
                x_17 = lean_ctor_get(x_1, 1);
                x_18 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF(x_16, x_2);
                x_19 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_countIncsCF(x_17, x_2);
                x_20 = lean_nat_dec_le(x_18, x_19);
                if x_20 == 0
                {
                    lean_dec(x_18);
                    return x_19;
                }
                else
                {
                    lean_dec(x_19);
                    return x_18;
                }
            }
            _ => {
                x_21 = lean_unsigned_to_nat(0);
                return x_21;
            }
        }
    }
    #[allow(unreachable_code)] unreachable!()
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instDecidableEqTy(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    x_4 = lean_box(x_3 as usize);
    return x_4;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy_beq___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: u8 = 0;
    let mut x_4: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy_beq(x_1, x_2);
    lean_dec(x_2);
    lean_dec(x_1);
    x_4 = lean_box(x_3 as usize);
    return x_4;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_nop_elim___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr) -> LeanObjPtr {
    let mut x_3: LeanObjPtr = std::ptr::null_mut();
    x_3 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBodyCF_ctorElim___redArg(x_1, x_2);
    return x_3;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countDecsCF_match__1_splitter(mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr, mut x_9: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_2) {
        0 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            x_10 = lean_ctor_get(x_2, 0);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_2, 1);
            lean_inc(x_11);
            x_12 = lean_ctor_get(x_2, 2);
            lean_inc(x_12);
            lean_dec(x_2);
            x_13 = lean_apply_4(x_5, x_10, x_11, x_12, x_3);
            return x_13;
        }
        1 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            x_14 = lean_ctor_get(x_2, 0);
            lean_inc(x_14);
            x_15 = lean_ctor_get(x_2, 1);
            lean_inc(x_15);
            lean_dec(x_2);
            x_16 = lean_apply_3(x_6, x_14, x_15, x_3);
            return x_16;
        }
        2 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            x_17 = lean_ctor_get(x_2, 0);
            lean_inc(x_17);
            x_18 = lean_ctor_get(x_2, 1);
            lean_inc(x_18);
            lean_dec(x_2);
            x_19 = lean_apply_3(x_4, x_17, x_18, x_3);
            return x_19;
        }
        3 => {
            lean_dec(x_9);
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_20 = lean_ctor_get(x_2, 0);
            lean_inc(x_20);
            x_21 = lean_ctor_get(x_2, 1);
            lean_inc(x_21);
            lean_dec(x_2);
            x_22 = lean_apply_3(x_7, x_20, x_21, x_3);
            return x_22;
        }
        4 => {
            lean_dec(x_9);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_23 = lean_apply_1(x_8, x_3);
            return x_23;
        }
        5 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_24 = lean_apply_1(x_9, x_3);
            return x_24;
        }
        _ => { unreachable!(); }
    }
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_FnBody_ctorElim(x_2, x_3, x_5);
    lean_dec(x_2);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_int_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_list_elim(mut x_2: LeanObjPtr, mut x_4: LeanObjPtr) -> LeanObjPtr {
    let mut x_5: LeanObjPtr = std::ptr::null_mut();
    x_5 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___redArg(x_2, x_4);
    return x_5;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim___boxed(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr) -> LeanObjPtr {
    let mut x_6: LeanObjPtr = std::ptr::null_mut();
    x_6 = lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_Ty_ctorElim(x_2, x_3, x_5);
    lean_dec(x_2);
    return x_6;
}

pub unsafe fn lp_almide_x2dperceus_x2dbelt___private_AlmidePerceusBelt_FnBody_0__AlmidePerceusBelt_countDecsCF_match__1_splitter___redArg(mut x_1: LeanObjPtr, mut x_2: LeanObjPtr, mut x_3: LeanObjPtr, mut x_4: LeanObjPtr, mut x_5: LeanObjPtr, mut x_6: LeanObjPtr, mut x_7: LeanObjPtr, mut x_8: LeanObjPtr) -> LeanObjPtr {
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
    match lean_obj_tag(x_1) {
        0 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_3);
            x_9 = lean_ctor_get(x_1, 0);
            lean_inc(x_9);
            x_10 = lean_ctor_get(x_1, 1);
            lean_inc(x_10);
            x_11 = lean_ctor_get(x_1, 2);
            lean_inc(x_11);
            lean_dec(x_1);
            x_12 = lean_apply_4(x_4, x_9, x_10, x_11, x_2);
            return x_12;
        }
        1 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_4);
            lean_dec(x_3);
            x_13 = lean_ctor_get(x_1, 0);
            lean_inc(x_13);
            x_14 = lean_ctor_get(x_1, 1);
            lean_inc(x_14);
            lean_dec(x_1);
            x_15 = lean_apply_3(x_5, x_13, x_14, x_2);
            return x_15;
        }
        2 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            x_16 = lean_ctor_get(x_1, 0);
            lean_inc(x_16);
            x_17 = lean_ctor_get(x_1, 1);
            lean_inc(x_17);
            lean_dec(x_1);
            x_18 = lean_apply_3(x_3, x_16, x_17, x_2);
            return x_18;
        }
        3 => {
            lean_dec(x_8);
            lean_dec(x_7);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_19 = lean_ctor_get(x_1, 0);
            lean_inc(x_19);
            x_20 = lean_ctor_get(x_1, 1);
            lean_inc(x_20);
            lean_dec(x_1);
            x_21 = lean_apply_3(x_6, x_19, x_20, x_2);
            return x_21;
        }
        4 => {
            lean_dec(x_8);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_22 = lean_apply_1(x_7, x_2);
            return x_22;
        }
        5 => {
            lean_dec(x_7);
            lean_dec(x_6);
            lean_dec(x_5);
            lean_dec(x_4);
            lean_dec(x_3);
            x_23 = lean_apply_1(x_8, x_2);
            return x_23;
        }
        _ => { unreachable!(); }
    }
}

unsafe fn _init_lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy___closed__0() -> LeanObjPtr {
    let mut x_1: LeanObjPtr = std::ptr::null_mut();
    x_1 = lean_alloc_closure(lp_almide_x2dperceus_x2dbelt_AlmidePerceusBelt_instBEqTy_beq___boxed as *mut _, 1, 0);
    return x_1;
}

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

