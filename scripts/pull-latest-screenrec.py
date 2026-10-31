#!/usr/bin/env python3
"""从手机拉取最新录屏到当前项目根的 tmp/screenrecorder/，供逐帧分析。

用法（项目根下）：scripts/pull-latest-screenrec.py [序号]
  无参数 = 最新一条；数字 N = 第 N 新（1 起，如 2 = 次新）。
落点：cwd 所在 git 仓库根的 tmp/screenrecorder/；不在 git 仓库时回退
cwd/tmp/screenrecorder/。
依赖：仅 Python 3 标准库 + adb 在 PATH。
输出：本地文件路径（一行），失败时错误信息退出码 1。
"""

import subprocess
import sys
from pathlib import Path

PHONE_DIR = "/sdcard/DCIM/ScreenRecorder/"


def project_root() -> Path:
    """cwd 所在 git 仓库根；不在仓库时回退 cwd。"""
    r = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True,
    )
    if r.returncode == 0:
        return Path(r.stdout.strip())
    return Path.cwd()


def main() -> int:
    rank = 1
    if len(sys.argv) > 1:
        try:
            rank = int(sys.argv[1])
            if rank < 1:
                raise ValueError
        except ValueError:
            print(f"无效序号: {sys.argv[1]!r}（需为 >=1 的整数）", file=sys.stderr)
            return 1

    listed = subprocess.run(
        ["adb", "shell", f"ls -t {PHONE_DIR}"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    if not listed:
        print(f"手机 {PHONE_DIR} 无录屏文件", file=sys.stderr)
        return 1
    if rank > len(listed):
        print(f"只有 {len(listed)} 条录屏，没有第 {rank} 新", file=sys.stderr)
        return 1

    name = listed[rank - 1]
    dest = project_root() / "tmp" / "screenrecorder"
    dest.mkdir(parents=True, exist_ok=True)
    local = dest / name
    subprocess.run(
        ["adb", "pull", f"{PHONE_DIR}{name}", str(local)],
        check=True, capture_output=True,
    )
    print(local)
    return 0


if __name__ == "__main__":
    sys.exit(main())
