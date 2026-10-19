# lints/

项目 lint 基础设施：**自定义 dylint 规则库** + **clippy/dylint 协同检查脚本** `check.py`。补充 clippy 无法覆盖的强制规范，统一 workspace/单包、多 target 的检查入口。

## 目录结构

```
lints/
├── rust-dylint/      # dylint 规则库（cdylib，7 条规则），详见 rust-dylint/README.md
└── scripts/
    └── check.py      # clippy + dylint 协同检查（自动推断 target/feature）
```

## 宿主项目接入

在项目根 `Cargo.toml`：

```toml
[workspace.metadata.dylint]
libraries = [{ path = "lints/rust-dylint" }]
```

> workspace 与单包均适用（单包的 `[workspace.metadata.dylint]` 隐式声明 workspace 根，无需额外 `[workspace]` 节）。规则细节见 `rust-dylint/README.md`。

## check.py 用法

| 命令 | 说明 |
|------|------|
| `python lints/scripts/check.py` | 全项目，人类可读输出（默认增量） |
| `python lints/scripts/check.py --json` | JSON 输出，供 rust-analyzer 解析 |
| `python lints/scripts/check.py -p PKG` | 单包检查 |

VS Code（rust-analyzer）：

```json
"rust-analyzer.check.overrideCommand": [
    "python3", "${workspaceFolder}/lints/scripts/check.py", "--json"
]
```

`check.py` 自动扫描 workspace 成员的 `[features]`，按 dioxus 平台关键词（`web`/`mobile`/`server`/`desktop`）推断每个 crate 的 `(target, feature)` 组合并分组检查。单包项目（无 `[workspace]` members）自动回退到检查根 package。

## 编译开销估算

`check.py` 对每个 target 组执行一次 clippy + 一次 dylint，而 dylint 又需先把规则库编译为 cdylib。下面给出**通用计算方法**，便于预估冷启动开销。公式为保守上界（假设各组 clippy/dylint-check 缓存完全隔离），实测可能因编译产物复用而更快。

### 变量

| 符号 | 含义 | 典型值 |
|------|------|--------|
| $G$ | 本次实际检查的 (target, feature) 组数 | 增量模式 $=1$；全量 $=N$ |
| $N$ | 项目全部 target 组数（各 crate 的 web/server/desktop/… 计数之和） | 单包桌面 1；fullstack 2 |
| $D_i$ | 组 $g_i$ 的依赖图包数——check.py 用 `-p A -p B` 联合检查组内所有 crate，编译的是各 crate 依赖图的**并集**（共享依赖只编一次），含 crate 自身 | 单 crate 组 ≈ 83；多 crate 组 ≈ 120–180 |
| $L$ | 规则库 `rust-dylint` 的依赖图包数（固定） | ≈ 36 |
| $c$ | dev profile 下单包 check 耗时 | ≈ 0.17 s |
| $r$ | release profile 下单包 build 耗时 | ≈ 0.67 s |

> $c, r$ 随机器标定，方法见文末。各组 $D_i$ 通常相近，估算时可简化为同一 $D$。

### 编译段数（compile passes）

每次 `check.py` 调用产生的 cargo 编译段：

$$P_{\text{冷}} = \underbrace{G}_{\text{clippy}} + \underbrace{1}_{\text{编译规则库}} + \underbrace{G}_{\text{dylint check}} = 2G + 1$$

$$P_{\text{热}} = 2G \quad (\text{规则库已缓存，省去编译段})$$

> 规则库 cdylib 一经编译，跨组、跨多次调用共享缓存，故"编译规则库"段恒为 $1$（冷）/$0$（热），与 $G$ 无关。
>
> 上述段数是"命令级"计数。每条 dylint 命令内部实际包含两步：先编译 lint 库为 cdylib（首次），再加载 cdylib 检查目标 crate。因此**"编译"动作次数** = clippy $G$ 次 + lint 库 1 次 + dylint check $G$ 次 = $2G+1$ 次（冷），与段数一致。

### 段数 vs 包数（常见误解）

> 误解："workspace 有 $N$ 个包，check.py 会一包一段跑 $N$ 次。"
> 实际：check.py 按 **target 组**遍历，同组的多个包用**一条命令** `-p A -p B` 合检。

check.py 全工作区模式先把包按 `(target, feature)` 分组（`groups.setdefault((target, feat), []).append(name)`），再遍历**组**而非包；每组只发起一条 clippy + 一条 dylint 命令，组内所有包以 `-p` 塞进同一条命令。故：

| 量 | 取决于 | 同组 1 包 → 同组 3 包 |
|----|--------|----------------------|
| 编译段数 $P$ | 组数 $G$ | **不变** |
| 编译单元数 $U$ / 耗时 $T$ | 组内依赖图**并集** $D$ | **变大** |

例子：3 个包 A/B/C —— 若同属 `(host, none)`，仅 1 组 → $P = 2\cdot1+1 = 3$（一条 `cargo clippy -p A -p B -p C`）；若分属 host / wasm / android 三组 → $P = 2\cdot3+1 = 7$。**决定段数的是组数，不是包数。**

推论：把多包 workspace 改为单包（target 组不变）**不会减少编译段数**；唯一能减段数的手段是砍掉某个 target 平台（减小 $G$）。

### 编译单元数（compilation units，包级）

cargo 以 `(package, profile, toolchain, --target)` 为独立编译单元。clippy 走项目默认工具链（如 stable），dylint check 走规则库锁定的 nightly，两者缓存隔离，**同依赖图需编译两次**：

$$U_{\text{冷}} = 2\sum_{i=1}^{G} D_i + L$$

各组 $D_i$ 相近时简化为：

$$U_{\text{冷}} \approx 2GD + L$$

### 时间估算

$$T_{\text{冷}} \approx 2c\sum_{i=1}^{G} D_i + rL \;\approx\; 2cGD + rL$$

$$T_{\text{热}} \approx 2cGD \quad (\text{省去 } rL)$$

### 示例

**单包桌面应用**（$G=N=1$，$D=83$，$L=36$）：

$$P_{\text{冷}} = 3,\quad U_{\text{冷}} = 2 \cdot 83 + 36 = 202,\quad T_{\text{冷}} \approx 2 \cdot 0.17 \cdot 83 + 0.67 \cdot 36 \approx 52\text{ s}$$

实测：clippy 14.2 s + 编译规则库 24.1 s + dylint check 3.8 s ≈ 42 s（dylint check 段命中部分依赖缓存，低于上界）。

**fullstack 项目**（$G=N=2$，$D\approx150$）：

$$P_{\text{冷}} = 5,\quad U_{\text{冷}} \approx 2 \cdot 2 \cdot 150 + 36 = 636,\quad T_{\text{冷}} \approx 2 \cdot 0.17 \cdot 2 \cdot 150 + 0.67 \cdot 36 \approx 126\text{ s}$$

> fullstack 通常含 web + server 两组，各组合检多个 crate，$D$ 大于单 crate。上例 $D=150$ 为典型值，实际取决于项目规模。

### 标定 $c, r$

在目标机器上对任意 crate 跑一次全冷 `check.py`，按段记录耗时 $t$ 与包数 $n$（`Locking N packages` 行）：

$$c = t_{\text{clippy}} / n_{\text{clippy}}, \qquad r = t_{\text{规则库}} / n_{\text{规则库}}$$

参考标定（bar，Windows）：$c \approx 14.16/83 = 0.17$ s，$r \approx 24.11/36 = 0.67$ s。
