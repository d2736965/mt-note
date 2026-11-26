#!/usr/bin/env python3
"""构建 release APK 并用 apksigner 签名。

签名成品输出到 tmp/apk/mt-note-v<版本>.apk（版本取自工作区根 Cargo.toml
的 [workspace.package] version，与关于页/安卓 versionName 同源）。

密码交互输入（getpass，不回显），不落命令行/历史/进程参数表：
    python scripts/build_release_apk.py -k <keystore路径>            # 构建 + 签名
    python scripts/build_release_apk.py -k <keystore路径> --sign-only <apk>  # 仅签名
alias 缺省时由 apksigner 自动选用 keystore 中唯一的 key（多条目则须 -a 显式指定）。
也可用 -p/-a 非交互传参（密码会进命令行，不推荐）。
"""
import argparse
import getpass
import glob
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APK_GLOB = str(
    ROOT / "target" / "dx" / "mt-note-mobile" / "release" / "android" / "**" / "*.apk"
)
OUTPUT_DIR = ROOT / "tmp" / "apk"


def read_version() -> str:
    """应用版本唯一真源：工作区根 Cargo.toml 的 [workspace.package] version。"""
    with (ROOT / "Cargo.toml").open("rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def find_apk(explicit: str | None) -> Path:
    if explicit:
        p = Path(explicit)
        if not p.is_file():
            sys.exit(f"✗ APK 不存在: {p}")
        return p
    apks = glob.glob(APK_GLOB, recursive=True)
    if not apks:
        sys.exit(
            f"✗ 未找到 release APK，请先执行构建。\n"
            f"  搜索路径: {APK_GLOB}"
        )
    return Path(max(apks, key=os.path.getmtime))


def build_release() -> None:
    """dx bundle 构建 release APK（仅 arm64-v8a）。"""
    print("• 构建 release APK: dx bundle -p mt-note-mobile --android -r --target aarch64-linux-android --package-types apk")
    result = subprocess.run(
        ["dx", "bundle", "-p", "mt-note-mobile", "--android", "--package-types", "apk", "-r", "--target", "aarch64-linux-android"],
        cwd=str(ROOT),
    )
    if result.returncode != 0:
        sys.exit(f"✗ 构建失败，退出码: {result.returncode}")
    print("✓ 构建完成")


def find_apksigner() -> str:
    """PATH 上没有时，退到 $ANDROID_HOME/build-tools/*/ 里按版本号取最新。"""
    found = shutil.which("apksigner")
    if found:
        return found
    sdk = Path(os.environ.get("ANDROID_HOME", Path.home() / "Android" / "Sdk"))
    candidates = sorted(
        sdk.glob("build-tools/*/apksigner"),
        key=lambda p: [int(x) for x in p.parent.name.split(".") if x.isdigit()],
    )
    if candidates:
        print(f"• PATH 无 apksigner，使用: {candidates[-1]}")
        return str(candidates[-1])
    sys.exit("✗ 未找到 apksigner：PATH 上没有，且 $ANDROID_HOME/build-tools/*/ 下也没有")


def sign_apk(apk: Path, keystore: Path, alias: str | None, password: str) -> Path:
    apksigner = find_apksigner()

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    output = OUTPUT_DIR / f"mt-note-v{read_version()}.apk"

    print(f"• keystore: {keystore}")
    print(f"• alias:    {alias or '(自动选用 keystore 唯一 key)'}")
    print(f"• 输入 APK: {apk}")
    print(f"• 输出 APK: {output}")

    cmd = [
        apksigner,
        "sign",
        "--ks", str(keystore),
        "--ks-pass", f"pass:{password}",
        "--key-pass", f"pass:{password}",
        "--out", str(output),
        str(apk),
    ]
    if alias:
        cmd[3:3] = ["--ks-key-alias", alias]
    result = subprocess.run(cmd)
    if result.returncode == 0:
        print(f"✓ 签名完成: {output}")
    else:
        sys.exit(result.returncode)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(
        description="构建并签名 mt-note release APK"
    )
    parser.add_argument(
        "-p", "--password", default=None, help="keystore 与 key 密码（缺省交互输入，不回显）"
    )
    parser.add_argument(
        "-k", "--ks", required=True, help="keystore 路径"
    )
    parser.add_argument(
        "-a", "--alias", default=None, help="key alias（keystore 仅一个 key 时可省略）"
    )
    parser.add_argument(
        "--sign-only",
        nargs="?",
        const=None,
        default=None,
        help="仅签名已有 APK（可选指定 APK 路径，默认自动找最新 release APK）",
    )
    args = parser.parse_args()

    keystore = Path(args.ks)
    if not keystore.is_file():
        sys.exit(f"✗ keystore 不存在: {keystore}")

    # 密码提示前置到构建前：启动即输入，输完构建自动进行，无需守候
    password = args.password or getpass.getpass("keystore 密码: ")

    if args.sign_only is not None:
        apk = find_apk(args.sign_only if args.sign_only else None)
        sign_apk(apk, keystore, args.alias, password)
    else:
        build_release()
        apk = find_apk(None)
        sign_apk(apk, keystore, args.alias, password)


if __name__ == "__main__":
    main()
