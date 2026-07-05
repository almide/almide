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

include!("perceus_belt_lean_p2.rs");
include!("perceus_belt_lean_p3.rs");
include!("perceus_belt_lean_p4.rs");
