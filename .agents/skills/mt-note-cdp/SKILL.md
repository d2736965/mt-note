---
name: mt-note-cdp
description: 当用户要求使用浏览器、打开网页、在线搜索、浏览网站、抓取网页数据，或通过 CDP 协议与浏览器/WebView 交互时使用。cdp-local 采用 launch 模式自管本机 Edge（Ubuntu，持久 profile 共享登录态）；cdp-remote 通过 cdp-proxy 连接 Android WebView 远程调试，直接用 MCP 查看渲染结果、控制台错误、网络请求等。（mt-note 项目自带版本）
---

# Skill: mt-note-cdp
# Browser Automation (Edge on Ubuntu)

## Overview

通过 chrome-devtools-mcp 控制浏览器完成任务。**当前环境仅此一台**：

| 项 | 值 |
|---|---|
| 系统 | Linux 桌面（以 Ubuntu 为例） |
| 浏览器 | Microsoft Edge（`/usr/bin/microsoft-edge-stable`，apt 官方源安装） |
| MCP | `cdp-local`（launch 模式，管本机 Edge） + `cdp-remote`（connect 模式，连 cdp-proxy） |

**核心原则**：自主管理浏览器连接，不依赖用户手动操作。

> 历史环境（Win11 + WSL、Termux）的流程已废弃删除。

## MCP 分工

| MCP | 连接方式 | 用途 |
|-----|---------|------|
| `cdp-local` | **launch 模式**（pipe 连接，无 TCP 端口） | 本机浏览器自动化：MCP 自启/自闭 Edge，持久 profile 跨会话共享登录态 |
| `cdp-remote` | connect 模式（`--browser-url` → 9228） | 远程 CDP 服务端（Android WebView 经 adb forward + cdp-proxy） |

### cdp-local launch 模式要点

- **零手动管理**：首次调用 `list_pages` 等工具时 MCP 自动拉起 Edge，会话结束自动关闭；无调试端口
- **持久 profile（显式指定）**：`--user-data-dir=~/.local/share/chrome-devtools-mcp/chrome-profile` —— 登录态、cookie 跨会话保留。用户在 MCP 浏览器窗口登录一次目标网站，之后 AI 打开即登录态。放在 `~/.local/share`（持久）而非 `~/.cache`（可能被清理工具误删）
- **严禁 `--isolated`**：临时 profile 每次清空，登录态全丢（仅隔离测试时用）
- **与日常浏览器并存**：单实例锁按 user-data-dir 隔离，互不影响，**永不杀用户日常浏览器**
- **Chromium 136+ 注意**：默认 profile 禁用 `--remote-debugging-port`（防调试端口窃取登录态）；launch 模式用 pipe + 专用 profile 天然免疫此限制

### MCP 配置（本仓库 .agents/mcp.json 已内置，见下；也可注册到所用 agent 工具的个人 MCP 配置）

```json
{
  "cdp-local": {
    "type": "stdio",
    "command": "npx",
    "args": ["-y", "chrome-devtools-mcp@latest",
             "--executablePath=/usr/bin/microsoft-edge-stable",   // 按本机浏览器路径调整
             "--user-data-dir=/home/<user>/.local/share/chrome-devtools-mcp/chrome-profile",
             "--no-usage-statistics"],
    "env": {}
  },
  "cdp-remote": {
    "type": "stdio",
    "command": "npx",
    "args": ["-y", "chrome-devtools-mcp@latest",
             "--browser-url=http://127.0.0.1:9228", "--no-usage-statistics"],
    "env": {}
  }
}
```

### cdp-remote 前置条件：cdp-proxy

操作 Android WebView 前，先确保 cdp-proxy 代理（9228）在运行：

```bash
cdp-proxy status          # exit 0 = 运行中，exit 1 = 未运行
cdp-proxy start           # 未运行时启动（源码 ~/pkgs/ts/cdp-proxy）
```

链路：手机 WebView → adb forward（9223）→ cdp-proxy（9228）→ cdp-remote。adb forward 的建立方法见 `mt-note-dioxus-reference` skill。

## Browser Readiness Check（浏览器就绪检测）

需要用浏览器时，**不要盲目重启**，先检测：

```
需要用浏览器 → 本轮对话已启动过?
  ├─ 是 → 直接操作（MCP 已连接）
  └─ 否 → 调用 list_pages
              ├─ 成功返回页面列表 → 直接操作（launch 模式未运行会自动拉起）
              └─ 报错/超时 → 检查 MCP 配置；cdp-remote 先查 cdp-proxy 是否运行
```

- ✅ 先试 `list_pages`，成功就直接用
- ❌ 不要未检测就杀浏览器进程（会丢用户已开页面）

## 标签页管理

**原则：复用已有标签页，避免无节制开新标签。**

```
list_pages → 检查可复用页面
  ├─ 目标 URL 已开 → select_page 切过去
  ├─ 同域名已开 → select_page + navigate_page 跳转
  └─ 都没有 → new_page（尽量避免）
```

- 同一网站只保留一个主要标签页，站内跳转用 `navigate_page`
- 抓详情页优先在列表页 `click` 链接；被弹窗拦截再 `navigate_page`
- 用完的标签页 `close_page` 及时关闭

## 常用操作

| 场景 | 方法 |
|------|------|
| 查看已开标签页 | `list_pages` |
| 页面导航 | `navigate_page type=url url=...` |
| 获取页面内容 | `take_snapshot`（注意：1.8.0+ 需显式传 `pageId`，从 `list_pages` 取） |
| 截图 | `take_screenshot filePath=...` |
| 点击 / 填表 | `click uid=...` / `fill uid=... value=...` |
| 等待加载 | `wait_for text=["关键词"]` |
| 控制台错误 | `list_console_messages types=["error","warn"]` |
| 网络请求 | `list_network_requests` → `get_network_request reqid=...` |
| 执行 JS | `evaluate_script` |
| 手机视口 | `emulate viewport=375x812x2,mobile,touch` |

## 数据提取策略（优先级从高到低）

- **A. Snapshot（首选）**：`take_snapshot` 拿结构化无障碍树，直接提取
- **B. 截图 + 图像分析**：Canvas/图片渲染页 snapshot 为空时，`take_screenshot` → `Read` 分析
- **C. 网络请求**：`list_network_requests resourceTypes=["xhr","fetch"]` → `get_network_request` 看响应；或 `evaluate_script` 直接 fetch API
- **D. 换 URL**：移动版不工作换 PC 版（通常更稳定）

## Network Rules（网络规则）

- **不访问无法直接加载的网站**：google.com、youtube.com、facebook.com、twitter.com 等一律不访问
- **搜索引擎**：中文 `cn.bing.com`，国际 `www.bing.com`，备选 `baidu.com`
- **技术文档**：直接访问官网 URL（docs.rs 等），不要经搜索引擎跳板

## Common Mistakes

| 错误 | 修正 |
|------|------|
| `Could not find Chrome executable` | launch 模式必须带 `--executablePath=<本机 Chromium 系浏览器路径，如 /usr/bin/microsoft-edge-stable>` |
| 连接 cdp-remote 失败 | cdp-proxy 没运行 → `cdp-proxy start`；或 adb forward 没建立 |
| take_snapshot 报错缺 pageId | 先 `list_pages` 取 pageId 再传 |
| Snapshot 只有 generic/image | Canvas 页 → 截图 + 图像分析 |
| API 返回空数据 | 检查请求参数（对照 `get_network_request`），直接调 API |
| 弹窗挡内容 | JS 移除或 Escape 关闭 |
| 每次操作都 `new_page` | 先 `list_pages` 复用 |
| 未检测就重启浏览器 | 先 `list_pages`，成功就直接用 |
