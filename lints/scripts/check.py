#!/usr/bin/env python3
"""依次运行 clippy 和 dylint，合并输出 rustc JSON 诊断供 rust-analyzer 解析。

## 用法

  python check.py        # 无参数：全工作区，人类可读输出，自动检测 target 并分组检查
  python check.py -p PKG # 单包：人类可读输出
  python check.py --json # 全工作区，JSON 输出，供 rust-analyzer 解析

## 自动 target 检测

未指定 --target 时，扫描 workspace 各 Cargo.toml 的 features：

  - 含 dioxus/web    → --target wasm32-unknown-unknown
  - 含 dioxus/mobile → --target aarch64-linux-android
  - 其余（SDK、库）   → host target（不传 --target）

## 自动 feature 检测

对非 host target，自动从 Cargo.toml 的 [features] 节检测应启用的 feature
（如 `web = ["dioxus/web"]` → `--features web`），避免 `--all-features` 同时启用
web 和 server feature 导致 mio 等依赖在 wasm32 上编译失败。

## 全工作区模式（VS Code rust-analyzer 默认调用）

1. 扫描 workspace members
2. 检测最近修改的 .rs 文件所属的 target 组（60s 窗口）
3. 仅对该组执行 clippy → dylint，其他组跳过
4. 无最近修改时回退到全量检查所有组

## 单包模式（-p，手动/CI 调用）

同样 clippy + dylint，适合提交前完整检查。

## VS Code 集成

  "rust-analyzer.check.overrideCommand": [
      "python3", "${workspaceFolder}/lints/scripts/check.py", "--json"
  ]

无需为不同 crate 配置不同命令。

## 设计决策

  - 默认人类可读输出，`--json` 切换为 JSON 格式供 rust-analyzer 解析
  - clippy 和 dylint 均不默认 `--all-features`：避免 Dioxus 全栈项目 web+server 同时启用
  - 非 host target 自动检测 feature：wasm32 → "web"，android → "mobile"
  - 所有目标均执行 clippy + dylint
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path


# ---- target 自动检测 ----

def cargo_workspace_root() -> Path:
    """从当前目录向上查找 Cargo.toml，返回 workspace 根目录路径。"""
    cwd = Path.cwd()
    for parent in [cwd, *cwd.parents]:
        manifest = parent / "Cargo.toml"
        if manifest.exists():
            # 确认这是 workspace root（有 [workspace] 声明）
            text = manifest.read_text(encoding="utf-8")
            if "[workspace]" in text:
                return parent
    return cwd  # 回退到当前目录


def _is_desktop_platform() -> bool:
    """判断当前系统是否为桌面平台（非 Android/iOS）。"""
    p = sys.platform
    if p == "win32" or p == "darwin":
        return True
    if p == "linux":
        import os
        if "ANDROID_ROOT" in os.environ or "ANDROID_DATA" in os.environ:
            return False
        return True
    return False


# dioxus 依赖关键词 → (target, 无 target 时的 fallback)
_DIOXUS_RULES: list[tuple[str, str | None, str | None]] = [
    ("dioxus/web",     "wasm32-unknown-unknown",    None),         # wasm32 固定目标
    ("dioxus/mobile",  "aarch64-linux-android",    None),         # android 固定目标
    ("dioxus/server",  None,                       None),         # host target
    ("dioxus/desktop", None,                       "x86_64-pc-windows-msvc"),  # 桌面→host，非桌面→win64
]


def detect_all_targets(crate_dir: Path) -> list[tuple[str | None, str | None]]:
    """扫描 Cargo.toml 的 [features]，返回该 crate 需要的所有 (target, feature) 组合。

    每个 feature 的依赖值匹配 dioxus 平台关键词，一条匹配 = 一次 lint 检查。
    一个 crate 可以有多个 target+feature 组合（如 fullstack crate 同时有 web 和 server）。
    """
    manifest = crate_dir / "Cargo.toml"
    if not manifest.exists():
        return [(None, None)]

    text = manifest.read_text(encoding="utf-8")
    # 提取每个 feature 的依赖值文本
    features = _parse_features(text)
    if not features:
        return [(None, None)]

    result: list[tuple[str | None, str | None]] = []
    for feat_name, feat_deps in features.items():
        for keyword, target, fallback_target in _DIOXUS_RULES:
            if keyword in feat_deps:
                if target is None and fallback_target is not None:
                    # desktop：根据当前平台选择 target
                    target = None if _is_desktop_platform() else fallback_target
                result.append((target, feat_name))
                break  # 每个 feature 只匹配第一个命中的规则

    if not result:
        result.append((None, None))
    return result


def _parse_features(text: str) -> dict[str, str]:
    """解析 Cargo.toml [features] 节，返回 {feature_name: deps_string}。

    deps_string 是 feature 等号右侧的完整内容（含多行数组合并）。
    """
    result: dict[str, str] = {}
    in_features = False
    current_name: str | None = None
    current_deps: list[str] = []

    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped != "[features]":
            in_features = stripped == "[features]"
            # 保存前一个 feature
            if current_name and current_deps:
                result[current_name] = " ".join(current_deps)
            current_name = None
            current_deps = []
            continue
        if stripped == "[features]":
            in_features = True
            continue
        if not in_features:
            continue

        if "=" in stripped:
            # 保存前一个 feature
            if current_name and current_deps:
                result[current_name] = " ".join(current_deps)
            name = stripped.split("=", 1)[0].strip()
            deps = stripped.split("=", 1)[1].strip()
            if name and not name.startswith("[") and name != "default":
                current_name = name
                current_deps = [deps]
        elif current_name is not None and stripped:
            # 多行数组的续行
            current_deps.append(stripped)

    # 保存最后一个
    if current_name and current_deps:
        result[current_name] = " ".join(current_deps)

    return result


def workspace_member_dirs(root: Path) -> list[tuple[str, Path]]:
    """从 workspace `Cargo.toml` 解析 members 列表。

    返回 (crate_name, crate_dir) 列表。
    支持 glob 通配符（如 `crates/*`）。

    单包回退：当无 [workspace] members 时（纯单包项目），
    把根 package 自身作为唯一成员返回，使脚本兼容单包应用。
    """
    import glob

    manifest = root / "Cargo.toml"
    if not manifest.exists():
        return []

    text = manifest.read_text(encoding="utf-8")

    # 提取 [workspace] 下的 members 列表
    patterns = []
    in_workspace = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped == "[workspace]":
            in_workspace = True
            continue
        if in_workspace and stripped.startswith("[") and stripped != "[workspace]":
            # 离开 [workspace] 节
            break
        if not in_workspace:
            continue
        if stripped.startswith("members"):
            # 解析 members = ["crates/{{project-name}}", ...] 形式
            # handles both single-line and multi-line
            pass
        if '"' in stripped:
            # 提取引号内的路径
            start = stripped.find('"')
            end = stripped.find('"', start + 1)
            if start != -1 and end != -1:
                rel_path = stripped[start + 1:end]
                patterns.append(rel_path)

    members = []
    for pat in patterns:
        # 展开 glob 模式（支持 crates/*）
        matched = glob.glob(str(root / pat), recursive=False)
        for m in matched:
            crate_dir = Path(m)
            if crate_dir.is_dir():
                name = crate_name(crate_dir)
                if name:
                    members.append((name, crate_dir))

    # 单包回退：无 [workspace] members 时（纯单包项目，或单成员空 workspace），
    # 把根 package 自身作为唯一成员，使本脚本兼容单包应用。
    if not members and "[package]" in text:
        name = crate_name(root)
        if name:
            members.append((name, root))

    return members


def crate_name(crate_dir: Path) -> str | None:
    """从 crate 的 Cargo.toml 读取 package name。"""
    manifest = crate_dir / "Cargo.toml"
    if not manifest.exists():
        return None
    text = manifest.read_text(encoding="utf-8")
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith('name = "'):
            return stripped.split('"')[1]
    return None


# ---- 命令构造与执行 ----

def run_check(cmd: list[str], passthrough: bool = False) -> int:
    """执行命令，返回退出码。

    passthrough=True（默认）：stdout/stderr 直接透传，人类可读。
    passthrough=False（--json）：capture stdout，stderr 忽略，JSON 输出。
    """
    if passthrough:
        return subprocess.run(cmd, stdout=sys.stdout, stderr=sys.stderr).returncode
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.stdout:
        sys.stdout.write(result.stdout)
    return result.returncode


def build_clippy(package: str | None, target: str | None,
                 features: str | None, extra: list[str],
                 json_output: bool = True) -> list[str]:
    """构造 clippy 命令。"""
    cmd = ["cargo", "clippy"]
    if package:
        cmd.extend(["-p", package])
    cmd.append("--all-targets")
    if json_output:
        cmd.append("--message-format=json")
    if target:
        cmd.extend(["--target", target])
    if features:
        cmd.extend(["--features", features])
    cmd.extend(extra)
    cmd.extend(["--", "--no-deps", "-Dwarnings"])
    return cmd


def build_dylint(package: str | None, target: str | None,
                 features: str | None, extra: list[str],
                 json_output: bool = True) -> list[str]:
    """构造 dylint 命令。

    dylint 语法：cargo dylint [dylint 选项] -- [cargo check 选项]
    -p 是 dylint 自带的选项（放 -- 前），--target / --features 放 -- 后。
    """
    cmd = ["cargo", "dylint", "--all", "--no-deps"]
    if package:
        cmd.extend(["-p", package])
    cmd.append("--")
    cmd.append("--all-targets")
    if json_output:
        cmd.append("--message-format=json")
    if target:
        cmd.extend(["--target", target])
    if features:
        cmd.extend(["--features", features])
    cmd.extend(extra)
    return cmd


# ---- 增量检测 ----

def detect_changed_group(
    member_map: dict[str, Path],
    pkg_targets: dict[str, list[tuple[str | None, str | None]]],
    recent_seconds: int = 60,
) -> tuple[str | None, str | None] | bool:
    """扫描 workspace 成员中最新的 .rs 文件，返回其所属 (target, feature) 组。

    一个 crate 有多个 target+feature 时，返回第一个（通常 web 在前）。

    返回:
        (target, feature)     → 仅检查该组
        False                 → 无最近修改，全量检查
    """
    best_mtime = 0.0
    best_key: tuple[str | None, str | None] = (None, None)

    for name, crate_dir in member_map.items():
        crate_mtime = newest_rs_mtime(crate_dir)
        if crate_mtime > best_mtime:
            best_mtime = crate_mtime
            targets = pkg_targets.get(name, [(None, None)])
            best_key = targets[0] if targets else (None, None)

    if best_mtime == 0.0:
        return False

    now = time.time()
    if now - best_mtime < recent_seconds:
        return best_key

    return False


def newest_rs_mtime(crate_dir: Path) -> float:
    """返回 crate 目录下最新 .rs 文件的 mtime，无文件返回 0.0。

    跳过 target/（构建产物）与 lints/（dylint 库源码），避免单包项目从
    根目录递归时扫描大量无关 .rs 文件（单包 root 会覆盖整个项目树）。
    """
    best = 0.0
    for f in crate_dir.rglob("*.rs"):
        # 跳过构建产物与 dylint 库目录（单包根递归时尤其重要）
        if "target" in f.parts or "lints" in f.parts:
            continue
        try:
            mtime = f.stat().st_mtime
            if mtime > best:
                best = mtime
        except OSError:
            pass
    return best


# ---- 主入口 ----

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("-p", "--package", default=None, help="要检查的包名")
    parser.add_argument("--target", default=None, help="编译目标（如 wasm32-unknown-unknown）")
    parser.add_argument("-f", "--features", default=None, help="要启用的 features（逗号分隔）")
    parser.add_argument("--json", action="store_true", default=False,
                        help="输出 JSON 格式（供 rust-analyzer 解析）")
    args, extra = parser.parse_known_args()

    root = cargo_workspace_root()
    members = workspace_member_dirs(root)

    # 为所有 workspace 成员自动推断 target+feature 组合
    # 一个 crate 可以有多个组合（如 fullstack crate: web + server）
    pkg_targets: dict[str, list[tuple[str | None, str | None]]] = {}
    for name, crate_dir in members:
        pkg_targets[name] = detect_all_targets(crate_dir)

    # 构建 name → dir 映射，供 feature 检测使用
    member_map = {name: crate_dir for name, crate_dir in members}

    exit_code = 0

    if args.package:
        # ---- 单包模式 ----
        # 遍历该包的所有 target+feature 组合
        targets = pkg_targets.get(args.package, [(None, None)])
        # 如果用命令行指定了 target 或 features，覆盖自动检测
        if args.target is not None or args.features is not None:
            targets = [(args.target, args.features)]

        for target, feat in targets:
            rc = run_check(build_clippy(args.package, target, feat, extra, json_output=args.json), passthrough=not args.json)
            if rc != 0:
                exit_code = 1
            rc = run_check(build_dylint(args.package, target, feat, extra, json_output=args.json), passthrough=not args.json)
            if rc != 0:
                exit_code = 1
    else:
        # ---- 全工作区模式（rust-analyzer 默认调用） ----
        # 按 (target, feature) 分组，同组的 crate 一起检查
        groups: dict[tuple[str | None, str | None], list[str]] = {}
        for name, targets in pkg_targets.items():
            for target, feat in targets:
                groups.setdefault((target, feat), []).append(name)

        # 检测最近修改的 crate，只检查它所在的 target 组
        changed_group = detect_changed_group(member_map, pkg_targets)

        for (target, feat), pkgs in groups.items():
            target_label = target or "host"

            if changed_group is not False and (target, feat) != changed_group:
                sys.stderr.write(f"[check] target={target_label} feat={feat or 'none'} skip (unchanged)\n")
                sys.stderr.flush()
                continue

            sys.stderr.write(f"[check] target={target_label} feat={feat or 'none'} pkgs={len(pkgs)}\n")
            sys.stderr.flush()

            features = args.features if args.features is not None else feat

            clippy = build_clippy(None, target, features, extra, json_output=args.json)
            for p in pkgs:
                clippy.insert(2, p)
                clippy.insert(2, "-p")

            rc = run_check(clippy, passthrough=not args.json)
            if rc != 0:
                exit_code = 1

            dylint = build_dylint(None, target, features, extra, json_output=args.json)
            for p in pkgs:
                dylint.insert(2, p)
                dylint.insert(2, "-p")

            rc = run_check(dylint, passthrough=not args.json)
            if rc != 0:
                exit_code = 1

    return exit_code


if __name__ == "__main__":
    sys.exit(main())