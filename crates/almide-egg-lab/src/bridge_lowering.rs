impl Bridge {
    /// Lower an `egg::RecExpr` back into `IrExpr`, allocating fresh
    /// VarIds through `vt` for any `compose` / `and-pred` markers that
    /// need to become real `IrExprKind::Lambda` nodes.
    ///
    /// The root of the `RecExpr` is taken to be its last node, which
    /// matches egg's post-order numbering after extraction.
    pub fn lower(
        &self,
        rec: &RecExpr<AlmideExpr>,
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        let nodes = rec.as_ref();
        assert!(!nodes.is_empty(), "cannot lower an empty RecExpr");
        let root = Id::from(nodes.len() - 1);
        self.lower_expr(rec, root, vt)
    }

    fn lower_expr(
        &self,
        rec: &RecExpr<AlmideExpr>,
        id: Id,
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        match &rec[id] {
            AlmideExpr::Symbol(s) => self.resolve_symbol_expr(s.as_str()),
            AlmideExpr::Num(n) => Ok(IrExpr {
                kind: IrExprKind::LitInt { value: *n },
                ty: Ty::Int,
                span: None, def_id: None,
            }),
            AlmideExpr::Map([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let ret_elem = lambda_ret_ty(&f).unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("map"),
                            def_id: None,
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_elem),
                    span: None, def_id: None,
                })
            }
            AlmideExpr::Filter([xs_id, p_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let p = self.lower_lambda_arg(rec, *p_id, &[elem_ty.clone()], vt)?;
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("filter"),
                            def_id: None,
                        },
                        args: vec![xs, p],
                        type_args: vec![],
                    },
                    ty: Ty::list(elem_ty),
                    span: None, def_id: None,
                })
            }
            AlmideExpr::Fold([xs_id, init_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let init = self.lower_expr(rec, *init_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let acc_ty = init.ty.clone();
                let f =
                    self.lower_lambda_arg(rec, *f_id, &[acc_ty.clone(), elem_ty], vt)?;
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("fold"),
                            def_id: None,
                        },
                        args: vec![xs, init, f],
                        type_args: vec![],
                    },
                    ty: acc_ty,
                    span: None, def_id: None,
                })
            }
            AlmideExpr::Lam(_)
            | AlmideExpr::Compose(_)
            | AlmideExpr::AndPred(_) => Err(LowerError::UnexpectedNode(
                "lambda-position marker in expression position".into(),
            )),

            AlmideExpr::MatrixMul([a, b]) =>
                self.lower_matrix_call(rec, "mul", &[*a, *b], vt),
            AlmideExpr::MatrixAdd([a, b]) =>
                self.lower_matrix_call(rec, "add", &[*a, *b], vt),
            AlmideExpr::MatrixScale([m, s]) =>
                self.lower_matrix_call(rec, "scale", &[*m, *s], vt),
            AlmideExpr::MatrixGelu([m]) =>
                self.lower_matrix_call(rec, "gelu", &[*m], vt),
            AlmideExpr::MatrixSoftmaxRows([m]) =>
                self.lower_matrix_call(rec, "softmax_rows", &[*m], vt),
            AlmideExpr::MatrixLinearRow([x, w, b]) =>
                self.lower_matrix_call(rec, "linear_row", &[*x, *w, *b], vt),
            AlmideExpr::MatrixLayerNormRows([x, g, be, e]) =>
                self.lower_matrix_call(rec, "layer_norm_rows", &[*x, *g, *be, *e], vt),

            AlmideExpr::MatrixFusedGemmBiasScaleGelu([a, b, bi, al]) =>
                self.lower_matrix_call(rec, "fused_gemm_bias_scale_gelu", &[*a, *b, *bi, *al], vt),
            AlmideExpr::MatrixAttentionWeights([q, kt, s]) =>
                self.lower_matrix_call(rec, "attention_weights", &[*q, *kt, *s], vt),
            AlmideExpr::MatrixScaledDotProductAttention([q, kt, v, s]) =>
                self.lower_matrix_call(rec, "scaled_dot_product_attention", &[*q, *kt, *v, *s], vt),
            AlmideExpr::MatrixPreNormLinear([x, g, be, e, w, b]) =>
                self.lower_matrix_call(rec, "pre_norm_linear", &[*x, *g, *be, *e, *w, *b], vt),
            AlmideExpr::MatrixLinearRowGelu([x, w, b]) =>
                self.lower_matrix_call(rec, "linear_row_gelu", &[*x, *w, *b], vt),
            AlmideExpr::MatrixMulScaled([a, s, b]) =>
                self.lower_matrix_call(rec, "mul_scaled", &[*a, *s, *b], vt),

            AlmideExpr::FlatMap([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                // flat_map's lambda returns List[U]; overall result is
                // List[U]. Try to recover U from the lambda's return
                // type; if that's unresolved, fall back to List[Int]
                // as a placeholder (saturation doesn't propagate types
                // through slot symbols).
                let ret_inner = lambda_ret_ty(&f)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("flat_map"),
                            def_id: None,
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_inner),
                    span: None, def_id: None,
                })
            }
            AlmideExpr::FilterMap([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                // filter_map's lambda returns Option[U]; overall
                // result is List[U]. Same fallback as flat_map.
                let ret_inner = lambda_ret_ty(&f)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("filter_map"),
                            def_id: None,
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_inner),
                    span: None, def_id: None,
                })
            }
            AlmideExpr::ComposeFold(_) | AlmideExpr::ComposeFlatmap(_)
            | AlmideExpr::ComposeMapFilter(_) | AlmideExpr::ComposeFmFold(_) => Err(LowerError::UnexpectedNode(
                "list-fusion marker in expression position".into(),
            )),
        }
    }

    /// Emit an `IrExprKind::Call` with `CallTarget::Module { matrix,
    /// <func> }`. Result type inherits from the first matrix-typed
    /// argument, which matches stdlib `matrix.<op>` signatures:
    /// every op takes a Matrix as arg[0] and returns a Matrix that
    /// shares its dtype. Children are lowered recursively so that
    /// fused RHS nodes reach back to their unfused leaves.
    fn lower_matrix_call(
        &self,
        rec: &RecExpr<AlmideExpr>,
        func: &str,
        children: &[Id],
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        let lowered: Result<Vec<IrExpr>, _> = children
            .iter()
            .map(|id| self.lower_expr(rec, *id, vt))
            .collect();
        let lowered = lowered?;
        let ty = lowered
            .iter()
            .map(|e| &e.ty)
            .find(|t| is_matrix_ty(t))
            .cloned()
            .unwrap_or(Ty::Matrix);
        Ok(IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("matrix"),
                    func: sym(func),
                    def_id: None,
                },
                args: lowered,
                type_args: vec![],
            },
            ty,
            span: None, def_id: None,
        })
    }

    /// Lower a node that sits in lambda position — i.e. the second
    /// arg of `map`/`filter` or the third of `fold`. Accepts
    /// `(lam _slot)`, bare `identity`, `compose`, and `and-pred`.
    ///
    /// `param_tys` describes the types the lambda expects. For map /
    /// filter this is a single element type; for fold it is
    /// `[acc_ty, elem_ty]`. Compose / and-pred are only legal in the
    /// unary case (map / filter), not fold.
    fn lower_lambda_arg(
        &self,
        rec: &RecExpr<AlmideExpr>,
        id: Id,
        param_tys: &[Ty],
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        match &rec[id] {
            AlmideExpr::Lam([slot_id]) => {
                let AlmideExpr::Symbol(s) = &rec[*slot_id] else {
                    return Err(LowerError::UnexpectedNode(
                        "(lam ...) child must be a slot symbol".into(),
                    ));
                };
                let slot_idx = parse_slot_index(s.as_str())?;
                let lam = self
                    .slots
                    .get(slot_idx)
                    .cloned()
                    .ok_or(LowerError::SlotOutOfRange(slot_idx))?;
                if !matches!(&lam.kind, IrExprKind::Lambda { .. }) {
                    return Err(LowerError::NotUnaryLambda);
                }
                Ok(lam)
            }
            AlmideExpr::Symbol(s) if s.as_str() == "identity" => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "identity marker requires exactly one param type".into(),
                    ));
                };
                Ok(build_identity_lambda(elem_ty.clone(), vt))
            }
            AlmideExpr::Symbol(s) => {
                // Bare slot symbol lifted as a lambda without the
                // `(lam ...)` wrapper — happens when the original IR
                // put a non-literal lambda-shaped slot straight into
                // the combinator. Fall back to slot lookup.
                let slot_idx = parse_slot_index(s.as_str())?;
                self.slots
                    .get(slot_idx)
                    .cloned()
                    .ok_or(LowerError::SlotOutOfRange(slot_idx))
            }
            AlmideExpr::Compose([g_id, f_id]) => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose marker only valid for unary lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[f_ret], vt)?;
                compose_lambdas_fresh(&f, &g, vt)
            }
            AlmideExpr::AndPred([p_id, q_id]) => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "and-pred marker only valid for unary lambda position".into(),
                    ));
                };
                let p = self.lower_lambda_arg(rec, *p_id, &[elem_ty.clone()], vt)?;
                let q = self.lower_lambda_arg(rec, *q_id, &[elem_ty.clone()], vt)?;
                compose_predicates_fresh(&p, &q, vt)
            }
            AlmideExpr::Map(_) | AlmideExpr::Filter(_) | AlmideExpr::Fold(_)
            | AlmideExpr::FlatMap(_) | AlmideExpr::FilterMap(_)
            | AlmideExpr::Num(_) => {
                Err(LowerError::UnexpectedNode(
                    "non-lambda node in lambda position".into(),
                ))
            }
            AlmideExpr::ComposeFold([g_id, f_id]) => {
                // fold position: param_tys = [acc_ty, elem_ty].
                // Build λ(acc, x). g(acc, f(x)).
                let [acc_ty, elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-fold marker only valid in fold's reducer position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[acc_ty.clone(), f_ret], vt)?;
                compose_map_into_fold_fresh(&f, &g, vt)
            }
            AlmideExpr::ComposeFlatmap([g_id, f_id]) => {
                // flat_map position: param_tys = [elem_ty]. Build
                // λx. list.flat_map(f(x), g).
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-flatmap marker only valid in flat_map's lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| Ty::list(elem_ty.clone()));
                let g_elem = f_ret.inner().cloned().unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[g_elem], vt)?;
                compose_flatmaps_fresh(&f, &g, vt)
            }
            AlmideExpr::ComposeMapFilter([p_id, f_id]) => {
                // filter_map position: param_tys = [elem_ty]. Build
                // λx. if p(f(x)) then some(f(x)) else none.
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-map-filter marker only valid in filter_map's lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let p = self.lower_lambda_arg(rec, *p_id, &[f_ret], vt)?;
                compose_map_filter_fresh(&f, &p, vt)
            }
            AlmideExpr::ComposeFmFold([g_id, fm_id]) => {
                // fold position: param_tys = [acc_ty, elem_ty]. Build
                // λ(acc, x). match fm(x) { some(y) ⇒ g(acc, y), none ⇒ acc }.
                let [acc_ty, elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-fm-fold marker only valid in fold's reducer position".into(),
                    ));
                };
                let fm = self.lower_lambda_arg(rec, *fm_id, &[elem_ty.clone()], vt)?;
                let fm_inner = lambda_ret_ty(&fm)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or_else(|| acc_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[acc_ty.clone(), fm_inner], vt)?;
                compose_filter_map_into_fold_fresh(&fm, &g, vt)
            }
            AlmideExpr::MatrixMul(_) | AlmideExpr::MatrixAdd(_)
            | AlmideExpr::MatrixScale(_) | AlmideExpr::MatrixGelu(_)
            | AlmideExpr::MatrixSoftmaxRows(_) | AlmideExpr::MatrixLinearRow(_)
            | AlmideExpr::MatrixLayerNormRows(_)
            | AlmideExpr::MatrixFusedGemmBiasScaleGelu(_)
            | AlmideExpr::MatrixAttentionWeights(_)
            | AlmideExpr::MatrixScaledDotProductAttention(_)
            | AlmideExpr::MatrixPreNormLinear(_)
            | AlmideExpr::MatrixLinearRowGelu(_)
            | AlmideExpr::MatrixMulScaled(_) => Err(LowerError::UnexpectedNode(
                "matrix node in lambda position".into(),
            )),
        }
    }

    fn resolve_symbol_expr(&self, name: &str) -> Result<IrExpr, LowerError> {
        let slot_idx = parse_slot_index(name)?;
        self.slots
            .get(slot_idx)
            .cloned()
            .ok_or(LowerError::SlotOutOfRange(slot_idx))
    }
}
