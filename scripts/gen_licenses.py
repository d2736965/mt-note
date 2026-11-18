#!/usr/bin/env python3
"""扫描全部依赖的开源许可并生成应用内「开源许可」页数据。

覆盖三路依赖：
  Rust   cargo-about 扫描（about.toml 的 accepted 白名单即兼容性门禁，
         白名单外许可会让 cargo-about 报错退出，本脚本随之失败），
         许可全文从 ~/.cargo registry 源码目录读取；
  NPM    crates/mobile/ts/package.json 的 dependencies，
         许可与全文读自 node_modules/<pkg>/；
  Kotlin target/dx 下 Android 工程的 debugRuntimeClasspath 完整依赖树
         （含传递依赖，经 gradlew 解析），许可读自 ~/.gradle 缓存的 POM，
         全文用本仓库内置的规范文本（Apache/MIT 取 LICENSE-*，BSD 内置）。

用法（项目根下）：scripts/gen_licenses.py
产物：crates/mobile/src/frontend/views/licenses/third_party.json
      （随源码提交；依赖变化后重跑本脚本即可）
门禁时机：crates/mobile/build.rs 在构建期自动调用本脚本（触发列表负责
      "唤醒"：Cargo.lock/Dioxus.toml/NPM 锁文件/本脚本/规范文本任一变化
      时 build.rs 重跑）；本脚本再对输入指纹戳对账，无变化 50ms 跳过，
      有变化才全量扫描。白名单外许可直接导致编译失败（build.rs 检查
      退出码）。手动重扫加 --force。
依赖：仅 Python 3 标准库 + cargo/cargo-about 在 PATH（gradle 不可达时
      Kotlin 段回退为直接解析 build.gradle.kts 的直接依赖并告警）。
"""

import hashlib
import json
import re
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

# ===== 兼容性门禁：与工作区根 about.toml 的 accepted 保持一致 =====
ALLOWED = {
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CC0-1.0",
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MPL-2.0",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
    "bzip2-1.0.6",
}

# 许可全文规范化：SPDX id -> 规范全文。gradle POM 只有许可名没有全文，
# 发布包缺 LICENSE 文件的 crate 也需兜底；规范文本存 scripts/licenses/
# <SPDX id>.txt（占位署名形式，与本仓库自身署名解耦），缺哪个 id 补哪个文件。
ROOT = Path(__file__).resolve().parent.parent
BUILTIN_TEXTS = {
    f.stem: f.read_text(encoding="utf-8")
    for f in (ROOT / "scripts/licenses").glob("*.txt")
}

# gradle POM <license><name> 常见写法 -> SPDX id
POM_NAME_TO_SPDX = {
    "the apache software license, version 2.0": "Apache-2.0",
    "apache license, version 2.0": "Apache-2.0",
    "apache license 2.0": "Apache-2.0",
    "apache 2.0": "Apache-2.0",
    "mit license": "MIT",
    "mit": "MIT",
    "bsd 3-clause \"new\" or \"revised\" license": "BSD-3-Clause",
    "bsd 3-clause": "BSD-3-Clause",
    "the bsd 3-clause license": "BSD-3-Clause",
    "bsd 2-clause": "BSD-2-Clause",
    "isc license": "ISC",
}

MOBILE_PKG = "mt-note-mobile"
OUT_PATH = ROOT / "crates/mobile/src/frontend/views/licenses/third_party.json"
TS_DIR = ROOT / "crates/mobile/ts"
# target/dx 下 Android gradle 工程目录（dx bundle 生成；gradlew 在 app/ 层）
ANDROID_DIR = ROOT / f"target/dx/{MOBILE_PKG}/debug/android"
GRADLE_DIR = ANDROID_DIR / "app"
# 输入指纹戳（target/ 不入库，构建间持久）：本脚本被 cargo/build.rs 频繁
# 唤醒（触发列表粗粒度），是否真需要全量扫描由这里的指纹对账裁决
STAMP_PATH = ROOT / "target" / "gen_licenses.stamp"

# 扫描结果依赖的全部输入（新增输入须同时进此列表与 build.rs 触发列表；
# 均为仓库内稳定文件，不含 target/ 构建产物）
STAMP_INPUTS = [
    ROOT / "Cargo.lock",
    ROOT / "Dioxus.toml",
    TS_DIR / "package.json",
    TS_DIR / "pnpm-lock.yaml",
    ROOT / "about.toml",
    ROOT / "scripts/gen_licenses.py",
    *sorted((ROOT / "scripts/licenses").glob("*.txt")),
]


def stamp_fingerprint() -> str:
    """全部输入的内容哈希；任何一份变化即判定需要重扫。"""
    lines = []
    for f in STAMP_INPUTS:
        try:
            digest = hashlib.sha256(f.read_bytes()).hexdigest()
        except OSError:
            digest = "<missing>"
        lines.append(f"{f.relative_to(ROOT)} {digest}")
    return "\n".join(lines) + "\n"


def _spdx_split(expr: str, op: str) -> list[str]:
    """按 AND/OR（op）切顶层项：括号内不切，返回各顶层片段。"""
    parts, depth, buf = [], 0, ""
    tokens = re.split(rf"(\s+{op}\s+)", expr)
    skip_next = False
    for tok in tokens:
        if skip_next:
            skip_next = False
            continue
        if re.fullmatch(rf"\s+{op}\s+", tok):
            if depth == 0:
                parts.append(buf)
                buf = ""
            else:
                buf += tok
        else:
            buf += tok
            depth += tok.count("(") - tok.count(")")
    parts.append(buf)
    return [p.strip() for p in parts if p.strip()]


def spdx_tokens(expr: str) -> list[str]:
    """收集 SPDX 表达式里的全部许可标识（剥括号、去 WITH 异常子句）。"""
    out: list[str] = []
    for m in re.finditer(r"[A-Za-z0-9.\-]+(?:\s+WITH\s+[A-Za-z0-9.\-]+)?", expr):
        t = m.group(0).strip()
        if t not in ("AND", "OR", "WITH") and not t.startswith(("AND", "OR")):
            out.append(t)
    return sorted(set(out))


def license_ok(expr: str) -> bool:
    """SPDX 表达式是否可接受：顶层 OR 任一分支全部成员（AND 连接的
    各项，括号表达式递归求值）都可接受即通过。"""
    for branch in _spdx_split(expr, "OR"):
        ok = True
        for term in _spdx_split(branch, "AND"):
            t = term.strip()
            if t.startswith("(") and t.endswith(")"):
                if not license_ok(t[1:-1]):
                    ok = False
                    break
            elif t not in ALLOWED:
                ok = False
                break
        if ok:
            return True
    return False


def read_license_texts(dir_: Path) -> str:
    """拼接目录下全部许可文件全文（去重，保持文件名序）。"""
    patterns = ("LICENSE*", "LICENCE*", "COPYING*", "UNLICENSE*", "NOTICE*")
    seen, texts = set(), []
    files = sorted(f for pat in patterns for f in dir_.glob(pat) if f.is_file())
    for f in files:
        try:
            t = f.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if t and t not in seen:
            seen.add(t)
            texts.append(t)
    return "\n\n-----\n\n".join(texts)


# ===== Rust（cargo-about）=====

def collect_rust() -> list[dict]:
    """cargo-about 扫描 Android 目标全部 Rust 依赖；门禁失败即抛错退出。"""
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tf:
        raw_path = tf.name
    r = subprocess.run(
        ["cargo", "about", "generate", "--format", "json", "-o", raw_path],
        cwd=ROOT, capture_output=True, text=True,
    )
    if r.returncode != 0:
        sys.stderr.write(r.stdout[-3000:] + r.stderr[-3000:])
        raise SystemExit("cargo-about 扫描失败：出现白名单外许可或环境错误（见上方日志）")

    data = json.loads(Path(raw_path).read_text(encoding="utf-8"))
    Path(raw_path).unlink(missing_ok=True)

    entries, no_text = [], []
    for item in data["crates"]:
        pkg = item["package"]
        # source 为空的包是 workspace 成员（我们自己的 crate），不进第三方清单
        if not pkg.get("source"):
            continue
        src_dir = Path(pkg["manifest_path"]).parent
        text = read_license_texts(src_dir)
        if not text:
            # 发布包不带 LICENSE 文件的 crate：选一个全部成员都有内置规范
            # 文本的 OR 分支兜底（MIT/Apache/BSD 等文本各持有者通用）
            expr = pkg.get("license") or ""
            for branch in _spdx_split(expr, "OR"):
                toks = [t.strip() for t in _spdx_split(branch, "AND")]
                if toks and all(t in BUILTIN_TEXTS for t in toks):
                    text = "\n\n-----\n\n".join(BUILTIN_TEXTS[t] for t in toks)
                    break
            if not text:
                no_text.append(f"{pkg['name']} {pkg['version']}")
        entries.append({
            "name": pkg["name"],
            "version": pkg["version"],
            "license": pkg.get("license") or "(未声明)",
            "repository": pkg.get("repository") or "",
            "text": text,
            "source": "rust",
        })
    if no_text:
        print(f"[警告] Rust 依赖未找到许可全文：{', '.join(no_text)}")
    return entries


# ===== NPM =====

def collect_npm() -> list[dict]:
    """ts/package.json 的运行时依赖：许可与全文读自 node_modules。"""
    pkg_json = json.loads((TS_DIR / "package.json").read_text(encoding="utf-8"))
    deps = pkg_json.get("dependencies", {})

    entries, missing = [], []
    for name in deps:
        real = (TS_DIR / "node_modules" / name).resolve()  # pnpm 符号链接跟进
        meta = real / "package.json"
        if not meta.is_file():
            missing.append(name)
            continue
        info = json.loads(meta.read_text(encoding="utf-8", errors="replace"))
        lic = info.get("license")
        if isinstance(lic, dict):
            lic = lic.get("type", "(未知)")
        elif isinstance(lic, list):
            lic = " OR ".join(
                x.get("type", "(未知)") if isinstance(x, dict) else str(x) for x in lic
            )
        lic = str(lic or "(未声明)")
        text = read_license_texts(real)
        repo = info.get("repository")
        repo_url = repo.get("url", "") if isinstance(repo, dict) else str(repo or "")
        entries.append({
            "name": name,
            "version": info.get("version", ""),
            "license": lic,
            "repository": repo_url,
            "text": text,
            "source": "npm",
        })
    if missing:
        print(f"[警告] NPM 依赖未装到 node_modules（先 pnpm install）：{', '.join(missing)}")
    return entries


# ===== Kotlin / gradle =====

def gradle_coordinates() -> list[str]:
    """完整依赖树坐标（含传递依赖）；gradle 不可达则回退解析直接依赖。"""
    coords: set[str] = set()
    if (GRADLE_DIR / "gradlew").is_file():
        r = subprocess.run(
            ["./gradlew", ":app:dependencies", "--configuration",
             "debugRuntimeClasspath", "--console", "plain", "-q"],
            cwd=GRADLE_DIR, capture_output=True, text=True,
        )
        if r.returncode == 0:
            # 只认依赖树行（"+--- g:a:v" / "|    \--- g:a:1 -> g:a:1.2"），
            # 避免 grep 全局输出误抓构建期插件等非运行时坐标；冲突箭头取
            # 末位（gradle 实际选用的版本）
            for line in r.stdout.splitlines():
                if not re.match(r"^\s*(?:[|+\\]|---)+", line) or "---" not in line:
                    continue
                seg = line.split("---", 1)[1].strip()
                ms = list(re.finditer(r"([\w.\-]+):([\w.\-]+):([\w.\-]+)", seg))
                if ms:
                    # 冲突行形如 "g:a:1.0 -> g:a:1.2"，取末位=gradle 实际选用版本
                    m = ms[-1]
                    coords.add(f"{m.group(1)}:{m.group(2)}:{m.group(3)}")
            return sorted(coords)
        print("[警告] gradlew 解析依赖树失败，回退为 build.gradle.kts 直接依赖")
    g = GRADLE_DIR / "build.gradle.kts"
    if not g.is_file():
        print("[警告] 未找到生成的 gradle 工程（先 dx bundle），Kotlin 依赖本次跳过")
        return []
    # 只认 implementation(...) 运行时依赖行，不抓 classpath/插件等构建期坐标
    for line in g.read_text(encoding="utf-8").splitlines():
        if "implementation(" not in line:
            continue
        m = re.search(r"\"([\w.\-]+):([\w.\-]+):([\w.\-]+)\"", line)
        if m:
            coords.add(m.group(0).strip('"'))
    return sorted(coords)


def pom_license(coord: str) -> str | None:
    """从 ~/.gradle 缓存的 POM 读许可名并映射 SPDX id；找不到返回 None。
    缓存布局：files-2.1/<group 带点单层目录>/<artifact>/<version>/<hash>/<a>-<v>.pom"""
    group, artifact, version = coord.split(":")
    base = Path.home() / ".gradle/caches/modules-2/files-2.1" / group / artifact / version
    poms = sorted(base.glob("*/*.pom"))
    if not poms:
        return None
    try:
        root = ET.parse(poms[0]).getroot()
    except ET.ParseError:
        return None
    lic = root.find("./licenses/license")
    if lic is None:
        return None
    name = (lic.findtext("name") or "").strip().lower()
    for key, spdx in POM_NAME_TO_SPDX.items():
        if key in name:
            return spdx
    return None


# 组织级许可映射：gradle 只把 AAR 实体化进缓存，POM 常缺失（元数据走
# 二进制缓存）；androidx/kotlin/material 等官方库的许可公开固定为 Apache-2.0，
# POM 拿不到时按组织前缀兜底（与各上游 POM 声明一致）。
GROUP_DEFAULT_SPDX = [
    ("androidx.", "Apache-2.0"),
    ("com.google.android.", "Apache-2.0"),
    ("com.google.", "Apache-2.0"),
    ("org.jetbrains.kotlin", "Apache-2.0"),
    ("org.jetbrains.kotlinx", "Apache-2.0"),
    ("org.jetbrains", "Apache-2.0"),
    ("org.jspecify", "Apache-2.0"),
]


def group_license(coord: str) -> str | None:
    for prefix, spdx in GROUP_DEFAULT_SPDX:
        if coord.startswith(prefix):
            return spdx
    return None


def collect_gradle() -> list[dict]:
    entries, unknown = [], []
    for coord in gradle_coordinates():
        spdx = pom_license(coord) or group_license(coord)
        if spdx is None:
            unknown.append(coord)
            text, lic = "", "(未声明)"
        else:
            lic = spdx
            text = BUILTIN_TEXTS.get(spdx, "")
        entries.append({
            "name": coord.rsplit(":", 1)[0],  # group:artifact（版本另列）
            "version": coord.split(":")[-1],
            "license": lic,
            "repository": "",
            "text": text,
            "source": "gradle",
        })
    if unknown:
        print("[警告] 以下 gradle 依赖在缓存中无 POM 或许可名未映射，未附许可："
              + ", ".join(unknown))
    return entries


# ===== 汇总与门禁 =====

def main() -> int:
    # 指纹对账：输入无变化则跳过扫描（--force 强制全量）。扫描失败不写戳，
    # 下次构建自动重试
    fingerprint = stamp_fingerprint()
    if "--force" not in sys.argv and STAMP_PATH.is_file() \
            and STAMP_PATH.read_text(encoding="utf-8") == fingerprint:
        print("[跳过] 许可输入无变化（--force 可强制重扫）")
        return 0

    rust = collect_rust()
    npm = collect_npm()
    gradle = collect_gradle()

    all_entries = sorted(rust + npm + gradle, key=lambda e: e["name"].lower())
    # 去重：同名同版本保留信息最全（text 更长）的一条
    by_key: dict[tuple, dict] = {}
    for e in all_entries:
        key = (e["source"], e["name"], e["version"])
        if key not in by_key or len(e["text"]) > len(by_key[key]["text"]):
            by_key[key] = e
    entries = sorted(by_key.values(), key=lambda e: (e["source"], e["name"]))

    # 全文池：同一份许可全文（数百依赖共用 MIT/Apache 等）只存一份，
    # 条目以 text_idx 引用，避免产物膨胀数 MB
    pool: list[str] = []
    pool_idx: dict[str, int] = {}
    out_entries = []
    for e in entries:
        t = e["text"]
        if t not in pool_idx:
            pool_idx[t] = len(pool)
            pool.append(t)
        d = {k: v for k, v in e.items() if k != "text"}
        d["text_idx"] = pool_idx[t]
        out_entries.append(d)
    doc = {"licenses": pool, "entries": out_entries}

    # 兼容性门禁：白名单外许可 → 失败退出，不写产物
    bad = {e["license"] for e in entries if not license_ok(e["license"])}
    if bad:
        print("[门禁] 发现白名单外的许可（见 about.toml accepted / 本脚本 ALLOWED）：")
        for b in sorted(bad):
            owners = [f"{e['source']}:{e['name']}" for e in entries if e["license"] == b]
            print(f"  {b}  <-  {', '.join(owners)}")
        return 1

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    body = json.dumps(doc, ensure_ascii=False, indent=1) + "\n"
    # 内容不变不重写：本脚本由 mobile/build.rs 在构建期调用，无条件重写会
    # 抖动 mtime，触发 include_str! 此产物的 crate 无谓重编译
    old = OUT_PATH.read_text(encoding="utf-8") if OUT_PATH.is_file() else None
    if old != body:
        OUT_PATH.write_text(body, encoding="utf-8")
    # 扫描成功才落指纹戳；门禁失败不落，下次构建自动重试
    STAMP_PATH.parent.mkdir(parents=True, exist_ok=True)
    STAMP_PATH.write_text(fingerprint, encoding="utf-8")
    counts = {}
    for e in entries:
        counts[e["source"]] = counts.get(e["source"], 0) + 1
    size_kb = OUT_PATH.stat().st_size // 1024
    print(f"[完成] 共 {len(entries)} 条（Rust {counts.get('rust', 0)} / "
          f"NPM {counts.get('npm', 0)} / gradle {counts.get('gradle', 0)}），"
          f"{size_kb} KB -> {OUT_PATH.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
