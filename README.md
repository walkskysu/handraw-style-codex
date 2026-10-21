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
git submodule update --init handraw-style
npm.cmd ci
npm.cmd run tauri -- dev
```

Tauri 自动启动 Vite 并在屏幕正中央打开桌面窗口。输入画面描述后点击「生成图片」，等待 Codex 完成后即可预览；生成失败时会弹出错误窗口，点击「知道了」或按 Esc 关闭后可以重试。再次生成期间和失败后保留上一张图片。

### 选择手绘风格、排版图型和主题色

工作台采用暖白背景、墨绿按钮和三栏布局：左侧输入描述并查看已选配置，中间的较宽区域浏览灵感图鉴，右侧展示作品、生成状态和本次描述。生成入口固定在创作区底部，重新生成或失败时保留上一张作品。

工作台固定在窗口内，不显示应用外层滚动条。灵感图鉴的风格、排版、主题色内容在各自标签内滚动，标题和类别标签保持可见；搜索或切换类别后回到内容顶部。小窗口中的创作区和预览面板仍可滚动访问，窄屏上下排列时在工作区内滚动，图鉴保留独立滚动区域。

图鉴通过「风格 / 排版 / 主题色」标签提供 **331 种手绘风格、167 种排版图型、36 种经典单色主题色**。每类全部预览在同一列表内连续滚动浏览，无需翻页，图片按需懒加载；共用一个按编号、名称或分类搜索的输入框。点击卡片立即选中，左侧同步显示名称和缩略图。每类最多选择一项；默认全部不选，点击左侧的「×」可单独清除，切换类别、滚动、搜索和失败重试都会保留选择。

点击左侧配置可切换到对应类别并滚动定位已选卡片（同时清空搜索）。窗口宽度小于 1200px 时优先显示创作与图鉴双栏，通过图鉴标题旁的「查看作品」打开预览侧边面板，入口同时显示生成状态；小于 640px 时创作区与图鉴上下排列。预览面板可通过关闭按钮、点击遮罩或 Esc 关闭，关闭后焦点返回入口。缩放窗口保留图鉴选择和生成结果。图鉴标签支持左右方向键、Home 和 End 切换。

选中项优先于描述中对应的要求：风格控制笔触与材质，排版控制构图与文字层级，单色主题控制配色（覆盖风格或排版示例中的其他颜色）。未选择的类别由描述决定。生成期间选择器暂停编辑。

图鉴数据和预览来自 [yang0/handraw-style](https://github.com/yang0/handraw-style)，与子模块锁定提交保持一致。图片打包在应用中，浏览选项不依赖网络；Rust 内置完整选项规范，校验编号后将规范与描述一起通过 stdin 传给 Codex。4000 字限制仅适用于用户描述。实际图像对规范的遵循程度取决于本地生图模型。

更新子模块后运行 `npm.cmd run catalog:sync` 同步 `src/data/generationCatalog.json`，并提交生成的数据。前端与 Rust 共用这份快照，测试会检查其与子模块的编号、图片和完整提示词一致；CI 会自动检出子模块。首次克隆也可以使用 `git clone --recurse-submodules`。

手动验收：切换各类 → 滚动到最后一张图并选中 → 搜索或滚动确认左侧摘要保留 → 点击摘要定位已选卡片 → 清除其中一类 → 提交生成 → 检查结果是否符合其余两类选择。另检查生成中不能重复提交或修改配置、失败保留旧图和描述，以及全部不选和只选一类；在 1659px、1100px 和 390px 窗口下检查布局、预览面板开关、Esc、Tab 焦点限制和连续滚动。

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
提示词最多 4000 个字符，图片文件最多 20 MB，解码缓冲区最多 64 MB。生成目录在请求结束时清理，生成成功后自动保存到本地图库；保存失败时保留预览与待保存数据，关闭应用前可重试或导出。

### 本地图库与导出

顶部可切换「创作工作台 / 我的图库」。生成成功后，原图与缩略图均写入并提交 SQLite 记录才显示「已保存到图库」。保存失败会保留预览，提供「重试保存」和「导出图片」；为避免丢失预览，需先保存成功，或导出后放弃本次图库保存，才能继续生成。失败的待保存内容只保留在当前进程内存中，请在关闭应用前处理。

图库按生成时间倒序排列，每页九张；支持按标题、原始提示词搜索和收藏筛选。详情包含大图、原始描述、三类配置及完整规范、实际提交提示词、图鉴内容 SHA-256 版本、尺寸、格式、文件大小和创建时间，可修改标题与标签。工具版本从 `codex --version` 的有效输出记录，读取失败或超时则留空；当前生成接口未返回可验证的模型名称，因此模型字段留空，不从本机凭据或整份配置推测。

「复用创作」回填描述与配置；已有编辑时先确认。未更换的选项继续使用旧作品保存的完整规范，即使新版图鉴已移除或修改该选项；更换后的选项使用当前图鉴。再次生成会创建新作品，不覆盖原作。复用条件不保证再次生成完全相同的图片。

「删除」移入最近删除，支持恢复；只有在最近删除中再次确认后才永久删除。「导出」打开系统保存对话框，默认定位用户的图片文件夹，也可以选择其他位置；可同时导出包含提示词和生成条件的 `.txt` 说明。取消对话框不会写入文件；已有同名说明文件时需选择新的文件名，避免静默覆盖。

图库根目录由 Tauri `app_local_data_dir()` 解析。当前应用标识为 `com.walkskysu.handraw`，Windows 默认目录为 `%LOCALAPPDATA%\com.walkskysu.handraw\`：

```text
gallery.db                 SQLite 索引与生成元数据（不含图片 Base64 或工具凭据）
images/YYYY-MM/<ID>.png     原始图片
thumbnails/<ID>.webp       最大 480 × 480 缩略图
staging/                   原子保存期间的临时文件
gallery.lock               进程锁，防止多个窗口同时清理和写入图库
```

数据库只保存相对文件路径。备份或迁移时先关闭应用，再复制整个目录（不要只复制数据库）。首次访问图库时清理异常退出遗留的临时文件、无索引文件和缺少原图/缩略图的记录；最近删除的文件保留到永久删除。图库与导出使用受限的 Rust 命令，不向前端开放任意本地文件读取能力。

图库验收：生成 → 确认已保存 → 关闭并重新打开 → 搜索/收藏 → 查看详情 → 复用并生成新作品 → 导出图片及说明 → 删除/恢复 → 确认永久删除。保存失败、写入回滚、启动恢复、分页、导出和历史规范复用由新增 Rust/前端测试覆盖；真实系统保存对话框仍需在桌面环境验收。

常见问题：

- **未找到 Codex / 无法启动**：检查 `PATH` 和 `codex.cmd exec --help`，安装后重启应用。
- **执行失败**：根据界面错误摘要检查 `codex.cmd login`、网络及工具配置。
- **模型要求更新 Codex**：遇到 `requires a newer version of Codex`（例如使用 `gpt-6-astra` 时）错误，弹窗会提示在终端运行 `npm.cmd install -g @openai/codex@latest`，升级完成后重启 Handraw 并重试。
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

# 浏览器布局回归测试（首次运行先安装 Chromium）
npx.cmd playwright install --no-shell chromium
npm.cmd run test:layout

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

前端测试覆盖浏览器限制、空/过长输入、真实 Tauri JS API 的模拟 IPC、等待状态、重复提交保护、失败重试、图片替换、显示失败、错误弹窗开关与关闭后焦点、版本升级提示及文本转义。jsdom 中模拟原生 dialog API；焦点限制与 Esc 关闭需在桌面 WebView 中手动验收。
Rust 测试使用本地假 CLI 子进程，覆盖标准输入原样传递、PNG 返回、退出码、无图片输出、找不到程序、超时回收、图片校验及 Windows 带空格路径的 `.cmd` 启动器。测试不调用付费模型，也不需要 Codex 登录。
模拟 IPC 和假 CLI 测试不等于实际模型或 WebView 的端到端测试。

`.github/workflows/ci.yml` 在 `windows-latest` 上执行前端测试、类型检查、Rust 测试和桌面编译。
手动验收：完成 Codex 生图配置 → 启动 `tauri dev` → 确认显示「桌面应用」→ 输入「一只窗边的水彩小猫」并提交 → 确认等待时不能重复提交、完成后显示真实图片 → 修改描述再次生成 → 确认预览更新。再检查断网/未登录时的错误及重试，并调整窗口大小检查布局。
窗口位置验收：分别启动开发版和打包后的桌面应用，确认主窗口首次显示时位于屏幕正中央；拖动窗口后关闭并重新启动，确认再次居中。使用多显示器或不同显示缩放比例时也检查居中位置。此行为由 Tauri 的 `app.windows[].center` 原生配置控制，需要在真实桌面环境中验收。

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
  src/gallery.rs         SQLite 图库、文件保存、恢复及导出
  src/main.rs            桌面程序入口
  capabilities/          主窗口权限
  icons/                 应用图标及 SVG 源文件
  tauri.conf.json        窗口、开发地址、安全策略和打包设置
```

新增桌面能力时在 Rust 中定义并注册命令，在 `src/services/` 中封装调用，再由 Vue 组件使用。仅在需要插件能力时扩展 capability 权限。
项目不包含绘图编辑器；本地图库数据独立于应用安装目录保存。

## 许可证

[MIT](LICENSE)

图鉴、预览图片与提示规范由 **yang0** 提供，原仓库：[yang0/handraw-style](https://github.com/yang0/handraw-style)。上游采用带署名要求的 MIT 许可证，全文随应用分发于 [public/handraw-style-LICENSE.txt](public/handraw-style-LICENSE.txt)。
