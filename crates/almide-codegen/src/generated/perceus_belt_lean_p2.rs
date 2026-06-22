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

