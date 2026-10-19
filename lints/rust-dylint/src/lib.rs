#![feature(rustc_private)]
#![deny(unused_must_use)]
#![forbid(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::allow_attributes,
    unfulfilled_lint_expectations,
)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

use rustc_errors::DiagDecorator;
use rustc_hir::{Arm, Expr, ExprKind, LetStmt, PatKind, QPath, Stmt, StmtKind};
use rustc_lint::{LateContext, LateLintPass, LintContext};

dylint_linting::dylint_library!();

rustc_session::declare_lint!(
    /// 禁止用下划线前缀变量静默丢弃任何值。
    ///
    /// `let _ = expr;` 和 `let _x = expr;` 是唯一能在所有 lint 规则下
    /// 静默丢弃非 `#[must_use]` 类型值的写法（如 `SendError`、`io::Error`）。
    /// 不得通过这种方式绕过错误处理。
    pub FORBID_UNDERSCORE_MUST_USE,
    Forbid,
    "禁止 let _ / let _x = 静默丢弃值"
);

rustc_session::declare_lint!(
    /// 非 wasm 目标禁止使用任何 mpsc channel（构造函数）。
    ///
    /// 桌面/移动端应使用 kameo Actor 模型管理并发状态。
    /// wasm32 目标允许 `futures_channel::mpsc`。
    pub FORBID_MPSC_CHANNEL,
    Forbid,
    "非 wasm 目标禁止使用 mpsc channel"
);

rustc_session::declare_lint!(
    /// 禁止丢弃 `Result` 或其错误值。
    ///
    /// 涵盖：`.ok()` / `.err()` / `.is_err()` / `.is_ok()` + `;` 丢弃；
    /// `Err(_) => ...` 用 `_` 丢弃错误值；`if result.is_err() {...}` 销毁错误值。
    /// 优先用 `?` 传播，无法传播时用 `if let Err(e) =` 显式处理。
    pub FORBID_DROPPED_RESULT,
    Forbid,
    "禁止丢弃 Result 或错误值"
);

rustc_session::declare_lint!(
    /// 禁止裸 `return;`（无返回值的早期退出）。
    ///
    /// 正常 Rust 函数末尾隐式返回 `()`，显式 `return;` 只在吞错误的
    /// 早期退出中出现。函数应返回 `Result`，用 `?` 传播错误。
    pub FORBID_BARE_RETURN,
    Forbid,
    "禁止裸 return;，错误应通过 ? 或 return Err(...) 传播"
);

rustc_session::declare_lint!(
    /// 禁止所有空的 `{}` 代码块。
    ///
    /// 空块 = 什么都没做，等于隐式丢弃状态。适用于 `if` / `if let`
    /// 的 then/else 分支和 `match` arm body。循环体（loop/while/for）
    /// 的空块不在此列——无限循环、自旋等待有意义。
    pub FORBID_EMPTY_BLOCK,
    Forbid,
    "禁止空的 {} 代码块，每个分支都必须有显式操作"
);

rustc_session::declare_lint_pass!(ForbidUnderscoreMustUse => [FORBID_UNDERSCORE_MUST_USE]);
rustc_session::declare_lint_pass!(ForbidMpscChannel => [FORBID_MPSC_CHANNEL]);
rustc_session::declare_lint_pass!(ForbidDroppedResult => [FORBID_DROPPED_RESULT]);
rustc_session::declare_lint_pass!(ForbidBareReturn => [FORBID_BARE_RETURN]);
rustc_session::declare_lint_pass!(ForbidEmptyBlock => [FORBID_EMPTY_BLOCK]);

rustc_session::declare_lint!(
    /// 禁止用户代码中用 `#[allow(unused_variables)]` / `#[expect(unused_variables)]` 绕过检查。
    ///
    /// `deny(unused_variables)` 下唯一能抑制未使用变量错误的方式。
    /// 外部 crate 宏展开中的 allow/expect 不受影响。
    pub FORBID_UNUSED_VARIABLES_ATTR,
    Forbid,
    "禁止用 allow/expect 绕过 unused_variables"
);

rustc_session::declare_lint!(
    /// 库 crate 禁止向控制台输出。
    ///
    /// 库不知道下游的运行环境——控制台输出可能污染下游的 TUI 界面、管道数据、日志格式。
    /// 优先用 `?` 传播错误。无法传播时用 `if let Err(e) =` 显式处理。
    /// 唯一例外：`tests/` 目录和 `#[cfg(test)]` 模块（编译为 bin 目标，自动豁免）。
    pub DENY_LIB_CONSOLE_OUTPUT,
    Deny,
    "库 crate 禁止使用 log/println/eprintln/dbg 等控制台输出"
);

rustc_session::declare_lint!(
    /// 禁止在非 js_bridge.rs 文件中使用 `#[expect(clippy::disallowed_methods)]`。
    ///
    /// `clippy::disallowed_methods` 禁止了 `dioxus::document::eval`，唯一合法豁免点
    /// 在 `js_bridge.rs` 的 `JsBridge` trait 实现中。其他文件不得绕过此限制。
    pub FORBID_DISALLOWED_METHODS_EXPECT,
    Forbid,
    "禁止在非 js_bridge.rs 中使用 #[expect(clippy::disallowed_methods)]"
);

rustc_session::declare_lint_pass!(ForbidUnusedVariablesAttr => [FORBID_UNUSED_VARIABLES_ATTR]);
rustc_session::declare_lint_pass!(ForbidDisallowedMethodsExpect => [FORBID_DISALLOWED_METHODS_EXPECT]);
rustc_session::declare_lint_pass!(DenyLibConsoleOutput => [DENY_LIB_CONSOLE_OUTPUT]);

#[unsafe(no_mangle)]
pub fn register_lints(sess: &rustc_session::Session, lint_store: &mut rustc_lint::LintStore) {
    dylint_linting::init_config(sess);
    lint_store.register_lints(&[
        FORBID_UNDERSCORE_MUST_USE,
        FORBID_MPSC_CHANNEL,
        FORBID_DROPPED_RESULT,
        FORBID_BARE_RETURN,
        FORBID_EMPTY_BLOCK,
        FORBID_UNUSED_VARIABLES_ATTR,
        FORBID_DISALLOWED_METHODS_EXPECT,
        DENY_LIB_CONSOLE_OUTPUT,
    ]);
    lint_store.register_late_pass(|_| Box::new(ForbidUnderscoreMustUse));
    lint_store.register_late_pass(|_| Box::new(ForbidMpscChannel));
    lint_store.register_late_pass(|_| Box::new(ForbidDroppedResult));
    lint_store.register_late_pass(|_| Box::new(ForbidBareReturn));
    lint_store.register_late_pass(|_| Box::new(ForbidEmptyBlock));
    lint_store.register_early_pass(|| Box::new(ForbidUnusedVariablesAttr));
    lint_store.register_early_pass(|| Box::new(ForbidDisallowedMethodsExpect));
    lint_store.register_pre_expansion_pass(|| Box::new(DenyLibConsoleOutput));
}

impl rustc_lint::EarlyLintPass for ForbidUnusedVariablesAttr {
    fn check_attribute(
        &mut self,
        cx: &rustc_lint::EarlyContext<'_>,
        attr: &rustc_ast::Attribute,
    ) {
        if attr.span.from_expansion() {
            return;
        }
        if !attr.has_name(rustc_span::symbol::sym::allow)
            && !attr.has_name(rustc_span::symbol::sym::expect)
        {
            return;
        }
        // 用 snippet 匹配 "unused_variables"
        let Ok(snippet) = cx.sess().source_map().span_to_snippet(attr.span) else {
            return;
        };
        if !snippet.contains("unused_variables") {
            return;
        }
        let name = if attr.has_name(rustc_span::symbol::sym::allow) {
            "allow"
        } else {
            "expect"
        };
        cx.opt_span_lint(
            FORBID_UNUSED_VARIABLES_ATTR,
            Some(attr.span),
            DiagDecorator(|diag| {
                diag.primary_message(format!(
                    "禁止用 #[{name}(unused_variables)] 绕过未使用变量检查"
                ));
                diag.help("删除未使用的变量而非用属性抑制；需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
            }),
        );
    }
}

impl rustc_lint::EarlyLintPass for ForbidDisallowedMethodsExpect {
    fn check_attribute(
        &mut self,
        cx: &rustc_lint::EarlyContext<'_>,
        attr: &rustc_ast::Attribute,
    ) {
        if attr.span.from_expansion() {
            return;
        }
        if !attr.has_name(rustc_span::symbol::sym::expect) {
            return;
        }
        let Ok(snippet) = cx.sess().source_map().span_to_snippet(attr.span) else {
            return;
        };
        if !snippet.contains("disallowed_methods") {
            return;
        }
        let filename = cx.sess().source_map().span_to_filename(attr.span);
        let path_str = filename.prefer_local_unconditionally().to_string();
        if path_str.contains("js_bridge") {
            return;
        }
        cx.opt_span_lint(
            FORBID_DISALLOWED_METHODS_EXPECT,
            Some(attr.span),
            DiagDecorator(|diag| {
                diag.primary_message(
                    "#[expect(clippy::disallowed_methods)] 仅允许在 js_bridge.rs 的 JsBridge trait 实现中使用"
                );
                diag.help("禁止直接使用裸 JS（dioxus::document::eval），必须通过 js-bridge-bindgen 生成的类型安全 API 调用");
            }),
        );
    }
}

impl<'tcx> LateLintPass<'tcx> for ForbidUnderscoreMustUse {
    fn check_local(&mut self, cx: &LateContext<'tcx>, local: &LetStmt<'tcx>) {
        if local.span.from_expansion() {
            return;
        }
        let Some(_init) = local.init else { return };

        match &local.pat.kind {
            PatKind::Wild => {}
            PatKind::Binding(_, _, ident, _) => {
                let name = ident.as_str();
                if !name.starts_with('_') || name.starts_with("__") {
                    return;
                }
            }
            _ => return,
        }

        cx.opt_span_lint(
            FORBID_UNDERSCORE_MUST_USE,
            Some(local.span),
            DiagDecorator(|diag| {
                diag.primary_message("禁止用 `let _` / `let _x` 静默丢弃值");
                diag.help("需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
            }),
        );
    }
}

impl<'tcx> LateLintPass<'tcx> for ForbidMpscChannel {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if is_wasm(cx) {
            return;
        }

        let ExprKind::Call(func, _) = expr.kind else { return };
        let ExprKind::Path(qpath) = &func.kind else { return };

        if is_mpsc_constructor(qpath) {
            cx.opt_span_lint(
                FORBID_MPSC_CHANNEL,
                Some(expr.span),
                DiagDecorator(|diag| {
                    diag.primary_message("非 wasm 目标禁止使用 mpsc channel 构造函数");
                    diag.help("桌面/移动端使用 kameo Actor 模型代替");
                }),
            );
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for ForbidDroppedResult {
    fn check_stmt(&mut self, cx: &LateContext<'tcx>, stmt: &Stmt<'tcx>) {
        let StmtKind::Semi(expr) = stmt.kind else { return };
        let ExprKind::MethodCall(segment, receiver, _args, _span) = &expr.kind else {
            return;
        };
        let method = segment.ident.as_str();
        let is_discard = matches!(method, "ok" | "err" | "is_err" | "is_ok");
        if !is_discard {
            return;
        }
        let receiver_ty = cx.typeck_results().expr_ty(receiver);
        if !type_is_result(cx.tcx, receiver_ty) {
            return;
        }
        cx.opt_span_lint(
            FORBID_DROPPED_RESULT,
            Some(stmt.span),
            DiagDecorator(|diag| {
                diag.primary_message(format!("禁止用 .{method}() 转换 Result 后丢弃"));
                diag.help("需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
            }),
        );
    }

    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        match &expr.kind {
            // match result { ... Err(_) => ... }
            ExprKind::Match(scrutinee, arms, _) => {
                let scrutinee_ty = cx.typeck_results().expr_ty(scrutinee);
                if !type_is_result(cx.tcx, scrutinee_ty) {
                    return;
                }
                for arm in *arms {
                    if arm_is_err_wildcard(arm) {
                        cx.opt_span_lint(
                            FORBID_DROPPED_RESULT,
                            Some(arm.pat.span),
                            DiagDecorator(|diag| {
                                diag.primary_message("禁止用 Err(_) 丢弃错误值");
                                diag.help("将 Err(_) 改为 Err(e)。需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                            }),
                        );
                    }
                }
            }
            // if result.is_err() { ... }
            ExprKind::If(cond, _then, _else) => {
                let ExprKind::MethodCall(segment, receiver, _args, _span) = &cond.kind else {
                    return;
                };
                if segment.ident.as_str() != "is_err" {
                    return;
                }
                let receiver_ty = cx.typeck_results().expr_ty(receiver);
                if !type_is_result(cx.tcx, receiver_ty) {
                    return;
                }
                cx.opt_span_lint(
                    FORBID_DROPPED_RESULT,
                    Some(expr.span),
                    DiagDecorator(|diag| {
                        diag.primary_message("禁止用 is_err() 丢弃错误值");
                        diag.help("需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                    }),
                );
            }
            _ => {}
        }
    }

    fn check_local(&mut self, cx: &LateContext<'tcx>, local: &LetStmt<'tcx>) {
        if local.els.is_none() {
            return;
        }
        if local.span.from_expansion() {
            return;
        }
        let Some(init) = local.init else { return };
        let init_ty = cx.typeck_results().expr_ty(init);
        if !type_is_result(cx.tcx, init_ty) {
            return;
        }
        if pat_is_ok_any(local.pat) {
            cx.opt_span_lint(
                FORBID_DROPPED_RESULT,
                Some(local.pat.span),
                DiagDecorator(|diag| {
                    diag.primary_message("禁止用 let Ok(...) = ... else 丢弃错误值");
                    diag.help("改用 if let Ok(x) = ... { ... } else { let Err(e) = ...; ... } 或 match，在 else/Err 分支中需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                }),
            );
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for ForbidBareReturn {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Ret(None) = &expr.kind {
            cx.opt_span_lint(
                FORBID_BARE_RETURN,
                Some(expr.span),
                DiagDecorator(|diag| {
                    diag.primary_message("禁止裸 return;");
                    diag.help("使用 ? 传播错误，或 return Err(...) / return Ok(...)；需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                }),
            );
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for ForbidEmptyBlock {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        match &expr.kind {
            ExprKind::If(_, then_block, else_expr) => {
                if expr_block_is_empty(then_block) {
                    cx.opt_span_lint(
                        FORBID_EMPTY_BLOCK,
                        Some(then_block.span),
                        DiagDecorator(|diag| {
                            diag.primary_message("禁止空的 {} 分支");
                            diag.help("每个分支都必须有显式操作，不能用空块静默丢弃；需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                        }),
                    );
                }
                if let Some(e) = else_expr {
                    lint_empty_else(cx, e);
                }
            }
            ExprKind::Match(_, arms, _) => {
                for arm in *arms {
                    // 跳过编译器脱糖生成的 arm（if let 的 else _ => {}、? 展开等）
                    if arm.body.span.desugaring_kind().is_some() {
                        continue;
                    }
                    if let ExprKind::Block(block, _) = &arm.body.kind
                        && block.stmts.is_empty()
                        && block.expr.is_none()
                    {
                        cx.opt_span_lint(
                            FORBID_EMPTY_BLOCK,
                            Some(arm.body.span),
                            DiagDecorator(|diag| {
                                diag.primary_message("禁止空的 match arm 体");
                                diag.help("每个 arm 都必须有显式操作，不能有静默行为；需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
                            }),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// 递归检查 `if/else` 链中所有空 then/else 块。
fn lint_empty_else(cx: &LateContext<'_>, expr: &Expr<'_>) {
    match &expr.kind {
        ExprKind::Block(block, _) if block.stmts.is_empty() && block.expr.is_none() => {
            cx.opt_span_lint(
                FORBID_EMPTY_BLOCK,
                Some(block.span),
                DiagDecorator(|diag| {
                    diag.primary_message("禁止空的 else {} 分支");
                    diag.help("每个分支都必须有显式操作，不能有静默行为");
                }),
            );
        }
        ExprKind::If(_, then_block, else_expr) => {
            if expr_block_is_empty(then_block) {
                cx.opt_span_lint(
                    FORBID_EMPTY_BLOCK,
                    Some(then_block.span),
                    DiagDecorator(|diag| {
                        diag.primary_message("禁止空的 {} 分支");
                        diag.help("每个分支都必须有显式操作，不能有静默行为");
                    }),
                );
            }
            if let Some(e) = else_expr {
                lint_empty_else(cx, e);
            }
        }
        _ => {}
    }
}

fn expr_block_is_empty(expr: &Expr<'_>) -> bool {
    if let ExprKind::Block(block, _) = &expr.kind {
        return block.stmts.is_empty() && block.expr.is_none();
    }
    false
}

fn arm_is_err_wildcard(arm: &Arm<'_>) -> bool {
    let PatKind::TupleStruct(qpath, pats, _) = &arm.pat.kind else {
        return false;
    };
    if !pats.iter().any(|p| matches!(p.kind, PatKind::Wild)) {
        return false;
    }
    let QPath::Resolved(_, path) = qpath else {
        return false;
    };
    path.segments
        .last()
        .is_some_and(|seg| seg.ident.as_str() == "Err")
}

fn pat_is_ok_any(pat: &rustc_hir::Pat<'_>) -> bool {
    let PatKind::TupleStruct(qpath, _pats, _) = &pat.kind else {
        return false;
    };
    let QPath::Resolved(_, path) = qpath else {
        return false;
    };
    path.segments
        .last()
        .is_some_and(|seg| seg.ident.as_str() == "Ok")
}

fn type_is_result(tcx: rustc_middle::ty::TyCtxt<'_>, ty: rustc_middle::ty::Ty<'_>) -> bool {
    let ty = ty.peel_refs();
    if let rustc_middle::ty::TyKind::Adt(adt_def, _) = ty.kind() {
        return tcx.is_diagnostic_item(rustc_span::symbol::sym::Result, adt_def.did());
    }
    false
}

fn is_wasm(cx: &LateContext<'_>) -> bool {
    cx.tcx.sess.target.arch == rustc_target::spec::Arch::Wasm32
}

fn is_mpsc_constructor(qpath: &QPath<'_>) -> bool {
    let QPath::Resolved(_, path) = qpath else { return false };
    let s = path
        .segments
        .iter()
        .map(|seg| seg.ident.as_str())
        .collect::<Vec<_>>()
        .join("::");
    s.contains("mpsc") && (s.ends_with("channel") || s.ends_with("unbounded_channel") || s.ends_with("unbounded"))
}

// ---------------------------------------------------------------------------
// DenyLibConsoleOutput
// ---------------------------------------------------------------------------

impl rustc_lint::EarlyLintPass for DenyLibConsoleOutput {
    fn check_mac(&mut self, cx: &rustc_lint::EarlyContext<'_>, mac: &rustc_ast::MacCall) {
        if mac.path.span.from_expansion() {
            return;
        }
        if is_bin_crate(cx) {
            return;
        }
        if is_in_tests_dir(cx, mac.path.span) {
            return;
        }
        let path = mac_path_to_string(&mac.path);
        if !is_console_output_macro(&path) {
            return;
        }
        cx.opt_span_lint(
            DENY_LIB_CONSOLE_OUTPUT,
            Some(mac.path.span),
            DiagDecorator(|diag| {
                diag.primary_message(format!("库 crate 禁止使用 {path}! 输出到控制台"));
                diag.help("需要处理错误情况的函数应返回 Result，优先用 ? 传播。无法传播时用 if let Err(e) = { ... }：一般错误 error!(...)，严重错误 unreachable!(...)");
            }),
        );
    }
}

fn is_bin_crate(cx: &rustc_lint::EarlyContext<'_>) -> bool {
    if cx.sess().opts.crate_types.iter().any(|ct| {
        matches!(ct, rustc_session::config::CrateType::Executable)
    }) {
        return true;
    }
    // dylint 不传 --crate-type，需通过环境变量检测二进制目标
    std::env::var("CARGO_BIN_NAME").is_ok()
}

fn is_in_tests_dir(cx: &rustc_lint::EarlyContext<'_>, span: rustc_span::Span) -> bool {
    let filename = cx.sess().source_map().span_to_filename(span);
    let path_str = filename.prefer_local_unconditionally().to_string();
    path_str.contains("/tests/") || path_str.contains("\\tests\\")
}

fn mac_path_to_string(path: &rustc_ast::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn is_console_output_macro(path: &str) -> bool {
    if matches!(path, "println" | "eprintln" | "print" | "eprint" | "dbg") {
        return true;
    }
    let leaf = path.rsplit("::").next().unwrap_or(path);
    matches!(leaf, "info" | "warn" | "error" | "debug" | "trace")
}