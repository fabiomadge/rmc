use super::NEEDLESS_RANGE_LOOP;
use crate::utils::visitors::LocalUsedVisitor;
use crate::utils::{
    contains_name, higher, is_integer_const, match_trait_method, multispan_sugg, path_to_local_id, paths, snippet,
    span_lint_and_then, sugg, SpanlessEq,
};
use clippy_utils::ty::has_iter_method;
use if_chain::if_chain;
use rustc_ast::ast;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::intravisit::{walk_expr, NestedVisitorMap, Visitor};
use rustc_hir::{BinOpKind, BorrowKind, Expr, ExprKind, HirId, Mutability, Pat, PatKind, QPath};
use rustc_lint::LateContext;
use rustc_middle::hir::map::Map;
use rustc_middle::middle::region;
use rustc_middle::ty::{self, Ty};
use rustc_span::symbol::{sym, Symbol};
use std::iter::Iterator;
use std::mem;

/// Checks for looping over a range and then indexing a sequence with it.
/// The iteratee must be a range literal.
#[allow(clippy::too_many_lines)]
pub(super) fn check<'tcx>(
    cx: &LateContext<'tcx>,
    pat: &'tcx Pat<'_>,
    arg: &'tcx Expr<'_>,
    body: &'tcx Expr<'_>,
    expr: &'tcx Expr<'_>,
) {
    if let Some(higher::Range {
        start: Some(start),
        ref end,
        limits,
    }) = higher::range(arg)
    {
        // the var must be a single name
        if let PatKind::Binding(_, canonical_id, ident, _) = pat.kind {
            let mut visitor = VarVisitor {
                cx,
                var: canonical_id,
                indexed_mut: FxHashSet::default(),
                indexed_indirectly: FxHashMap::default(),
                indexed_directly: FxHashMap::default(),
                referenced: FxHashSet::default(),
                nonindex: false,
                prefer_mutable: false,
            };
            walk_expr(&mut visitor, body);

            // linting condition: we only indexed one variable, and indexed it directly
            if visitor.indexed_indirectly.is_empty() && visitor.indexed_directly.len() == 1 {
                let (indexed, (indexed_extent, indexed_ty)) = visitor
                    .indexed_directly
                    .into_iter()
                    .next()
                    .expect("already checked that we have exactly 1 element");

                // ensure that the indexed variable was declared before the loop, see #601
                if let Some(indexed_extent) = indexed_extent {
                    let parent_id = cx.tcx.hir().get_parent_item(expr.hir_id);
                    let parent_def_id = cx.tcx.hir().local_def_id(parent_id);
                    let region_scope_tree = cx.tcx.region_scope_tree(parent_def_id);
                    let pat_extent = region_scope_tree.var_scope(pat.hir_id.local_id);
                    if region_scope_tree.is_subscope_of(indexed_extent, pat_extent) {
                        return;
                    }
                }

                // don't lint if the container that is indexed does not have .iter() method
                let has_iter = has_iter_method(cx, indexed_ty);
                if has_iter.is_none() {
                    return;
                }

                // don't lint if the container that is indexed into is also used without
                // indexing
                if visitor.referenced.contains(&indexed) {
                    return;
                }

                let starts_at_zero = is_integer_const(cx, start, 0);

                let skip = if starts_at_zero {
                    String::new()
                } else if visitor.indexed_mut.contains(&indexed) && contains_name(indexed, start) {
                    return;
                } else {
                    format!(".skip({})", snippet(cx, start.span, ".."))
                };

                let mut end_is_start_plus_val = false;

                let take = if let Some(end) = *end {
                    let mut take_expr = end;

                    if let ExprKind::Binary(ref op, ref left, ref right) = end.kind {
                        if let BinOpKind::Add = op.node {
                            let start_equal_left = SpanlessEq::new(cx).eq_expr(start, left);
                            let start_equal_right = SpanlessEq::new(cx).eq_expr(start, right);

                            if start_equal_left {
                                take_expr = right;
                            } else if start_equal_right {
                                take_expr = left;
                            }

                            end_is_start_plus_val = start_equal_left | start_equal_right;
                        }
                    }

                    if is_len_call(end, indexed) || is_end_eq_array_len(cx, end, limits, indexed_ty) {
                        String::new()
                    } else if visitor.indexed_mut.contains(&indexed) && contains_name(indexed, take_expr) {
                        return;
                    } else {
                        match limits {
                            ast::RangeLimits::Closed => {
                                let take_expr = sugg::Sugg::hir(cx, take_expr, "<count>");
                                format!(".take({})", take_expr + sugg::ONE)
                            },
                            ast::RangeLimits::HalfOpen => format!(".take({})", snippet(cx, take_expr.span, "..")),
                        }
                    }
                } else {
                    String::new()
                };

                let (ref_mut, method) = if visitor.indexed_mut.contains(&indexed) {
                    ("mut ", "iter_mut")
                } else {
                    ("", "iter")
                };

                let take_is_empty = take.is_empty();
                let mut method_1 = take;
                let mut method_2 = skip;

                if end_is_start_plus_val {
                    mem::swap(&mut method_1, &mut method_2);
                }

                if visitor.nonindex {
                    span_lint_and_then(
                        cx,
                        NEEDLESS_RANGE_LOOP,
                        expr.span,
                        &format!("the loop variable `{}` is used to index `{}`", ident.name, indexed),
                        |diag| {
                            multispan_sugg(
                                diag,
                                "consider using an iterator",
                                vec![
                                    (pat.span, format!("({}, <item>)", ident.name)),
                                    (
                                        arg.span,
                                        format!("{}.{}().enumerate(){}{}", indexed, method, method_1, method_2),
                                    ),
                                ],
                            );
                        },
                    );
                } else {
                    let repl = if starts_at_zero && take_is_empty {
                        format!("&{}{}", ref_mut, indexed)
                    } else {
                        format!("{}.{}(){}{}", indexed, method, method_1, method_2)
                    };

                    span_lint_and_then(
                        cx,
                        NEEDLESS_RANGE_LOOP,
                        expr.span,
                        &format!("the loop variable `{}` is only used to index `{}`", ident.name, indexed),
                        |diag| {
                            multispan_sugg(
                                diag,
                                "consider using an iterator",
                                vec![(pat.span, "<item>".to_string()), (arg.span, repl)],
                            );
                        },
                    );
                }
            }
        }
    }
}

fn is_len_call(expr: &Expr<'_>, var: Symbol) -> bool {
    if_chain! {
        if let ExprKind::MethodCall(ref method, _, ref len_args, _) = expr.kind;
        if len_args.len() == 1;
        if method.ident.name == sym!(len);
        if let ExprKind::Path(QPath::Resolved(_, ref path)) = len_args[0].kind;
        if path.segments.len() == 1;
        if path.segments[0].ident.name == var;
        then {
            return true;
        }
    }

    false
}

fn is_end_eq_array_len<'tcx>(
    cx: &LateContext<'tcx>,
    end: &Expr<'_>,
    limits: ast::RangeLimits,
    indexed_ty: Ty<'tcx>,
) -> bool {
    if_chain! {
        if let ExprKind::Lit(ref lit) = end.kind;
        if let ast::LitKind::Int(end_int, _) = lit.node;
        if let ty::Array(_, arr_len_const) = indexed_ty.kind();
        if let Some(arr_len) = arr_len_const.try_eval_usize(cx.tcx, cx.param_env);
        then {
            return match limits {
                ast::RangeLimits::Closed => end_int + 1 >= arr_len.into(),
                ast::RangeLimits::HalfOpen => end_int >= arr_len.into(),
            };
        }
    }

    false
}

struct VarVisitor<'a, 'tcx> {
    /// context reference
    cx: &'a LateContext<'tcx>,
    /// var name to look for as index
    var: HirId,
    /// indexed variables that are used mutably
    indexed_mut: FxHashSet<Symbol>,
    /// indirectly indexed variables (`v[(i + 4) % N]`), the extend is `None` for global
    indexed_indirectly: FxHashMap<Symbol, Option<region::Scope>>,
    /// subset of `indexed` of vars that are indexed directly: `v[i]`
    /// this will not contain cases like `v[calc_index(i)]` or `v[(i + 4) % N]`
    indexed_directly: FxHashMap<Symbol, (Option<region::Scope>, Ty<'tcx>)>,
    /// Any names that are used outside an index operation.
    /// Used to detect things like `&mut vec` used together with `vec[i]`
    referenced: FxHashSet<Symbol>,
    /// has the loop variable been used in expressions other than the index of
    /// an index op?
    nonindex: bool,
    /// Whether we are inside the `$` in `&mut $` or `$ = foo` or `$.bar`, where bar
    /// takes `&mut self`
    prefer_mutable: bool,
}

impl<'a, 'tcx> VarVisitor<'a, 'tcx> {
    fn check(&mut self, idx: &'tcx Expr<'_>, seqexpr: &'tcx Expr<'_>, expr: &'tcx Expr<'_>) -> bool {
        if_chain! {
            // the indexed container is referenced by a name
            if let ExprKind::Path(ref seqpath) = seqexpr.kind;
            if let QPath::Resolved(None, ref seqvar) = *seqpath;
            if seqvar.segments.len() == 1;
            then {
                let index_used_directly = path_to_local_id(idx, self.var);
                let indexed_indirectly = {
                    let mut used_visitor = LocalUsedVisitor::new(self.cx, self.var);
                    walk_expr(&mut used_visitor, idx);
                    used_visitor.used
                };

                if indexed_indirectly || index_used_directly {
                    if self.prefer_mutable {
                        self.indexed_mut.insert(seqvar.segments[0].ident.name);
                    }
                    let res = self.cx.qpath_res(seqpath, seqexpr.hir_id);
                    match res {
                        Res::Local(hir_id) => {
                            let parent_id = self.cx.tcx.hir().get_parent_item(expr.hir_id);
                            let parent_def_id = self.cx.tcx.hir().local_def_id(parent_id);
                            let extent = self.cx.tcx.region_scope_tree(parent_def_id).var_scope(hir_id.local_id);
                            if indexed_indirectly {
                                self.indexed_indirectly.insert(seqvar.segments[0].ident.name, Some(extent));
                            }
                            if index_used_directly {
                                self.indexed_directly.insert(
                                    seqvar.segments[0].ident.name,
                                    (Some(extent), self.cx.typeck_results().node_type(seqexpr.hir_id)),
                                );
                            }
                            return false;  // no need to walk further *on the variable*
                        }
                        Res::Def(DefKind::Static | DefKind::Const, ..) => {
                            if indexed_indirectly {
                                self.indexed_indirectly.insert(seqvar.segments[0].ident.name, None);
                            }
                            if index_used_directly {
                                self.indexed_directly.insert(
                                    seqvar.segments[0].ident.name,
                                    (None, self.cx.typeck_results().node_type(seqexpr.hir_id)),
                                );
                            }
                            return false;  // no need to walk further *on the variable*
                        }
                        _ => (),
                    }
                }
            }
        }
        true
    }
}

impl<'a, 'tcx> Visitor<'tcx> for VarVisitor<'a, 'tcx> {
    type Map = Map<'tcx>;

    fn visit_expr(&mut self, expr: &'tcx Expr<'_>) {
        if_chain! {
            // a range index op
            if let ExprKind::MethodCall(ref meth, _, ref args, _) = expr.kind;
            if (meth.ident.name == sym::index && match_trait_method(self.cx, expr, &paths::INDEX))
                || (meth.ident.name == sym::index_mut && match_trait_method(self.cx, expr, &paths::INDEX_MUT));
            if !self.check(&args[1], &args[0], expr);
            then { return }
        }

        if_chain! {
            // an index op
            if let ExprKind::Index(ref seqexpr, ref idx) = expr.kind;
            if !self.check(idx, seqexpr, expr);
            then { return }
        }

        if_chain! {
            // directly using a variable
            if let ExprKind::Path(QPath::Resolved(None, path)) = expr.kind;
            if let Res::Local(local_id) = path.res;
            then {
                if local_id == self.var {
                    self.nonindex = true;
                } else {
                    // not the correct variable, but still a variable
                    self.referenced.insert(path.segments[0].ident.name);
                }
            }
        }

        let old = self.prefer_mutable;
        match expr.kind {
            ExprKind::AssignOp(_, ref lhs, ref rhs) | ExprKind::Assign(ref lhs, ref rhs, _) => {
                self.prefer_mutable = true;
                self.visit_expr(lhs);
                self.prefer_mutable = false;
                self.visit_expr(rhs);
            },
            ExprKind::AddrOf(BorrowKind::Ref, mutbl, ref expr) => {
                if mutbl == Mutability::Mut {
                    self.prefer_mutable = true;
                }
                self.visit_expr(expr);
            },
            ExprKind::Call(ref f, args) => {
                self.visit_expr(f);
                for expr in args {
                    let ty = self.cx.typeck_results().expr_ty_adjusted(expr);
                    self.prefer_mutable = false;
                    if let ty::Ref(_, _, mutbl) = *ty.kind() {
                        if mutbl == Mutability::Mut {
                            self.prefer_mutable = true;
                        }
                    }
                    self.visit_expr(expr);
                }
            },
            ExprKind::MethodCall(_, _, args, _) => {
                let def_id = self.cx.typeck_results().type_dependent_def_id(expr.hir_id).unwrap();
                for (ty, expr) in self.cx.tcx.fn_sig(def_id).inputs().skip_binder().iter().zip(args) {
                    self.prefer_mutable = false;
                    if let ty::Ref(_, _, mutbl) = *ty.kind() {
                        if mutbl == Mutability::Mut {
                            self.prefer_mutable = true;
                        }
                    }
                    self.visit_expr(expr);
                }
            },
            ExprKind::Closure(_, _, body_id, ..) => {
                let body = self.cx.tcx.hir().body(body_id);
                self.visit_expr(&body.value);
            },
            _ => walk_expr(self, expr),
        }
        self.prefer_mutable = old;
    }
    fn nested_visit_map(&mut self) -> NestedVisitorMap<Self::Map> {
        NestedVisitorMap::None
    }
}
