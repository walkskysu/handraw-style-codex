# Handraw

使用 **Vue 3 + TypeScript + Vite + Tauri 2** 搭建的手绘风格生图工作台。
输入提示词，点击「生成图片」，应用调用本机 Codex CLI 并在预览窗口显示图片。

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

Tauri 自动启动 Vite 并打开桌面窗口。输入画面描述后点击「生成图片」，等待 Codex 完成后即可预览；生成失败时会显示原因，可以直接重试。再次生成期间和失败后保留上一张图片。

### Codex 生图准备

在运行 Handraw 的同一用户下安装并登录 [Codex CLI](https://learn.chatgpt.com/docs/codex-cli)：

```powershell
npm.cmd install -g @openai/codex
codex.cmd login
codex.cmd exec --help
```

安装后重启 Handraw，让应用获得新的 `PATH`。Windows 支持原生 `codex.exe` 和 npm 安装的 `codex.cmd`；其他系统从 `PATH` 查找 `codex`。
本地 Codex 还需配置可用的图片生成工具或技能及其所需凭据；仅安装 CLI 不保证具备生图能力。应用复用本机 Codex 的登录、模型和工具配置，不内置 API Key。
建议先在终端确认 Codex 能根据描述生成并保存 PNG。若使用的技能调用外部图像 API，请按该技能的说明配置凭据和网络访问。

后端使用[非交互模式](https://learn.chatgpt.com/docs/non-interactive-mode)，等价命令为：

```text
codex --ask-for-approval never exec --skip-git-repo-check --sandbox workspace-write --config sandbox_workspace_write.network_access=true --color never -
```

每次在独立临时目录执行，提示词通过 UTF-8 标准输入传入，要求 Codex 将实际生成图片保存为 `result.png`。不会将提示词拼进 shell 命令，也不会关闭 Codex 文件沙箱；允许联网，以便生图技能访问图像服务。需要交互授权的工具调用会失败，应用会展示 CLI 的错误摘要。
Rust 在后台线程等待，最多运行 10 分钟；Windows 超时会终止包含 npm 启动器在内的进程树。PNG 校验通过后以 data URL 返回前端，无需开放任意本地文件读取权限。
提示词最多 4000 个字符，图片文件最多 20 MB，解码缓冲区最多 64 MB。生成目录在请求结束时清理，预览仅保存在当前窗口内存中，关闭应用不会保留历史。

常见问题：

- **未找到 Codex / 无法启动**：检查 `PATH` 和 `codex.cmd exec --help`，安装后重启应用。
- **执行失败**：根据界面错误摘要检查 `codex.cmd login`、网络及工具配置。
- **未输出 result.png**：Codex 未实际生成文件；确认当前模型和生图工具可用，并支持保存 PNG。
- **超时 / 图片过大或损坏**：缩短描述或要求较小尺寸后重试。

无需 Rust 的浏览器预览：

```powershell
npm.cmd run dev
```

访问 `http://127.0.0.1:1420`。页面明确标注「浏览器预览」，可编辑提示词，但生成按钮禁用；只有桌面应用能调用本地 Codex。
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

前端测试覆盖浏览器限制、空/过长输入、真实 Tauri JS API 的模拟 IPC、等待状态、重复提交保护、失败重试、图片替换、显示失败及文本转义。
Rust 测试使用本地假 CLI 子进程，覆盖标准输入原样传递、PNG 返回、退出码、无图片输出、找不到程序、超时回收、图片校验及 Windows 带空格路径的 `.cmd` 启动器。测试不调用付费模型，也不需要 Codex 登录。
模拟 IPC 和假 CLI 测试不等于实际模型或 WebView 的端到端测试。

`.github/workflows/ci.yml` 在 `windows-latest` 上执行前端测试、类型检查、Rust 测试和桌面编译。
手动验收：完成 Codex 生图配置 → 启动 `tauri dev` → 确认显示「桌面应用」→ 输入「一只窗边的水彩小猫」并提交 → 确认等待时不能重复提交、完成后显示真实图片 → 修改描述再次生成 → 确认预览更新。再检查断网/未登录时的错误及重试，并调整窗口大小检查布局。

## 项目结构

```text
src/
  App.vue                首页、输入状态与错误反馈
  main.ts                Vue 入口
  styles.css             手绘风格与响应式布局
  services/imageGeneration.ts  生图 IPC 及输入校验
tests/                   Vitest + Vue Test Utils 测试
src-tauri/
  src/lib.rs             异步 Rust 命令注册
  src/generation.rs      Codex 进程、图片读取及测试
  src/main.rs            桌面程序入口
  capabilities/          主窗口权限
  icons/                 应用图标及 SVG 源文件
  tauri.conf.json        窗口、开发地址、安全策略和打包设置
```

新增桌面能力时在 Rust 中定义并注册命令，在 `src/services/` 中封装调用，再由 Vue 组件使用。仅在需要插件能力时扩展 capability 权限。
项目不包含绘图编辑器或持久化功能，便于后续按业务需求扩展。

## 许可证

[MIT](LICENSE)
