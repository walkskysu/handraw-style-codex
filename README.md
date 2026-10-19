# Handraw

使用 **Vue 3 + TypeScript + Vite + Tauri 2** 搭建的手绘风格桌面应用基础框架。
包含响应式首页、浏览器预览、Vue → Rust 命令调用，以及可在 Windows 上运行的自动化测试。

## Windows 环境准备

仅运行前端和前端测试需要 **Node.js 22.12+（推荐 Node.js 24 LTS）及 npm**。
开发或构建桌面应用还需要：

1. [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选「使用 C++ 的桌面开发」及 Windows SDK。
2. [Rust](https://www.rust-lang.org/tools/install)，使用 stable MSVC 工具链（通常为 `stable-x86_64-pc-windows-msvc`）。
3. [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)，Windows 10/11 通常已安装。

完整说明见 [Tauri 官方前置依赖文档](https://v2.tauri.app/start/prerequisites/)。安装后重新打开 PowerShell，确认 `node --version`、`cargo --version` 和 `rustc --version` 可以执行。

## 本地运行

在项目根目录打开 PowerShell：

```powershell
npm.cmd ci
npm.cmd run tauri -- dev
```

Tauri 自动启动 Vite 并打开桌面窗口。输入名字后点击「打个招呼」，页面通过 `invoke('greet')` 调用 Rust，显示「你好，…！欢迎来到 Handraw。」。

无需 Rust 的浏览器预览：

```powershell
npm.cmd run dev
```

访问 `http://127.0.0.1:1420`。页面会明确标注「浏览器预览」，使用本地问候逻辑，不调用桌面 API。
开发端口固定为 `1420`，被占用时会报错；修改时需同步修改 `vite.config.ts` 和 `src-tauri/tauri.conf.json`。

## 测试与构建

```powershell
# 一次性运行前端组件与 IPC 适配测试，不依赖 Rust 或桌面窗口
npm.cmd test

# TypeScript 检查及前端生产构建（输出 dist/）
npm.cmd run build

# Rust 命令单元测试，需要完整桌面开发环境
npm.cmd run test:rust

# 编译 Windows 调试应用，不生成安装程序
npm.cmd run tauri -- build --debug --no-bundle

# 编译发布版本并生成 NSIS 安装程序
npm.cmd run tauri -- build
```

安装程序输出到 `src-tauri/target/release/bundle/nsis/`。首次构建会下载 Rust 依赖及相关打包工具，需要联网。
PowerShell 使用 `npm.cmd` 可避免 `npm.ps1` 被执行策略拦截；其他系统可使用 `npm`。

前端测试覆盖浏览器降级、空输入、真实 Tauri JS API 的模拟 IPC、等待状态、重复提交保护、失败重试及文本转义。
Rust 测试覆盖中文问候、空白修剪与空名字拒绝。模拟 IPC 测试不等于真实 WebView 的端到端测试。

`.github/workflows/ci.yml` 在 `windows-latest` 上执行前端测试、类型检查、Rust 测试和桌面编译。
可按以下步骤手动验收桌面链路：启动 `tauri dev` → 确认显示「桌面应用」→ 输入名字并提交 → 确认返回 Rust 问候 → 调整窗口大小检查布局。

## 项目结构

```text
src/
  App.vue                首页、输入状态与错误反馈
  main.ts                Vue 入口
  styles.css             手绘风格与响应式布局
  services/greeting.ts   Tauri IPC 及浏览器预览适配
tests/                   Vitest + Vue Test Utils 测试
src-tauri/
  src/lib.rs             Rust 命令注册及单元测试
  src/main.rs            桌面程序入口
  capabilities/          主窗口权限
  icons/                 应用图标及 SVG 源文件
  tauri.conf.json        窗口、开发地址、安全策略和打包设置
```

新增桌面能力时在 Rust 中定义并注册命令，在 `src/services/` 中封装调用，再由 Vue 组件使用。仅在需要插件能力时扩展 capability 权限。
项目不包含绘图编辑器或持久化功能，便于后续按业务需求扩展。

## 许可证

[MIT](LICENSE)
