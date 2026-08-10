# DeskAide

DeskAide 是一个常驻 Windows 桌面的电子 AI 助手入口。它以可拖动的透明助手形象驻留在桌面，通过独立的 Assistant 面板连接 OpenAI-Compatible 文字模型，并允许用户把选中文字或多个外部窗口中可访问的文字作为本次提问的上下文。

当前项目基于 Tauri 2、Svelte 5 和 Rust，仍处于早期开发阶段，仅实现并验证 Windows。

> DeskAide 不会持续读取屏幕或监控其他应用。只有用户主动选择上下文并发送时，它才会尝试读取相应文字；发送前可以查看和编辑从外部窗口生成的上下文草稿。

## 功能概览

### 桌面助手与外观

- 透明、无边框、始终置顶的助手形象窗口；支持拖动、多显示器工作区定位和位置持久化。
- 单击助手形象或按 `Ctrl + Shift + Space` 打开 Assistant 面板。
- Assistant 面板支持紧凑/展开、临时置顶、失焦隐藏，以及跟随助手形象重新定位。
- 支持浅色与深色主题，选择会保存在本机并在下次启动时恢复。
- 内置机器人助手、女性助手和东亚女性助手三套形象，当前默认使用东亚女性助手。
- 助手形象由 Manifest 驱动，支持静态图片和自动播放、静音循环的 WebM 视频资源。

### 对话与模型

- 支持 OpenAI-Compatible `/v1/chat/completions` 流式与非流式响应。
- 真正发送当前对话的多轮文字历史，支持新建对话和停止生成。
- 有用户消息的对话自动保存在本机；历史抽屉支持继续对话、重命名和删除。
- 每条历史记录保存其模型 Profile；载入时会尝试恢复原模型，但应用启动后仍默认进入空白新对话。
- 支持多个模型 Profile、默认模型、对话中切换模型和手动连接测试。
- API Key 按 Profile 隔离保存在 Windows Credential Manager，不进入普通配置文件或前端 IPC 响应。
- 内置不发送网络请求的 Mock Provider，未配置真实模型时也可离线体验和开发。
- Rust 后端向前端提供模型能力和上下文窗口大小，尚不可用的上下文选项会显示明确原因。

### 可编辑的桌面上下文

- 保留原有“当前选中文字”流程：记录 Assistant 激活前的外部窗口，仅在用户勾选并发送后读取选区。
- 可以枚举当前可见的外部顶层窗口，任意多选并分别采集可访问文字。
- 每个窗口上下文以摘要卡片展示；单击后可在独立编辑窗口中预览、修改将要发送的完整草稿。
- 通过 Windows UI Automation 尽力获取选中文字或指定窗口公开的可访问文字，单次采集设有 3 秒超时。
- 发送前按当前模型的上下文预算截断文字；某项采集失败不会阻止普通提问。
- 窗口文字、选中文字和临时草稿只参与本次模型请求，不会写入历史对话正文或自动沿用到下一轮。

## 环境要求

- Windows 10 1803 或更新版本
- Microsoft Edge WebView2 Runtime
- Microsoft C++ Build Tools（安装“使用 C++ 的桌面开发”工作负载）
- Rust stable MSVC toolchain（项目最低 Rust 版本为 1.85）
- Node.js 24+ 和 npm 11+

完整的 Tauri Windows 前置条件见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)。

## 快速开始

安装依赖并启动开发环境：

```powershell
npm install
npm run tauri -- dev
```

开发模式会同时启动 Vite（`http://localhost:1420`）和 Tauri。首次启动只显示桌面助手形象；拖动形象可以改变位置，单击形象或按 `Ctrl + Shift + Space` 打开 Assistant。

### 配置模型

1. 打开 Assistant 右上角的“设置”。
2. 在“模型配置”下新建 OpenAI-Compatible Profile。
3. 填写 Base URL、Model ID、上下文长度和最大输出 Token；需要鉴权时填写 API Key。
4. 保存后手动点击“测试连接”，再将该 Profile 设为当前模型。

Base URL 可以是 Provider 根路径（例如 `https://api.longcat.chat/openai`），也可以是以 `/v1` 结尾的 API 地址。示例：

```text
Base URL: https://api.longcat.chat/openai
Model ID: LongCat-2.0
上下文长度: 1048576
最大输出 Token: 131072
```

### 添加窗口上下文

1. 打开输入区旁的上下文菜单。
2. 若要读取激活前窗口中的选区，勾选“当前选中文字”。
3. 若要添加其他窗口，刷新窗口列表并多选目标窗口，然后生成上下文草稿。
4. 单击摘要卡片可检查和修改草稿；确认后随问题一起发送。

## 检查与构建

提交前可运行完整检查：

```powershell
npm run format:check
npm run lint
npm run check
npm run test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

生成不依赖本地 Vite 服务、可直接双击运行的 Debug EXE：

```powershell
npm run build:debug
```

输出位于 `target/debug/deskaide.exe`。不要把 `tauri dev` 运行期间生成的临时 EXE 当作独立程序：它仍会连接本地 Vite 服务，关闭开发服务器后会出现 `localhost:1420` 连接失败。

生成 Release 版本和 NSIS 安装包：

```powershell
npm run tauri -- build
```

## 项目结构

```text
apps/desktop/              Svelte 前端与 Tauri Windows 应用
  src/assistant/           对话、历史记录和上下文编辑界面
  src/avatar/              形象资源包加载与静态/视频渲染
  src/settings/            主题、形象和模型 Profile 设置
  src-tauri/               窗口协调、IPC、凭据和本地持久化
crates/assistant-core/     共享请求、消息、上下文和事件类型
crates/ai-provider/        Mock 与 OpenAI-Compatible ModelProvider
crates/context-core/       ContextProvider 与 PlatformIntegration
crates/platform-windows/   Windows 窗口追踪与 UI Automation 能力
docs/                      架构、形象资源格式和 Windows 限制
```

更多设计说明：

- [当前架构](docs/architecture.md)
- [静态助手形象资源包格式（v1）](docs/avatar-pack-format.md)
- [Windows 已知限制](docs/windows-limitations.md)

## 隐私与安全边界

- 激活 Assistant 时只为“当前选中文字”记录前一窗口的元数据和 UI Automation 元素引用，不立即读取文字。
- 用户明确添加外部窗口后才读取该窗口文字，并且发送时使用用户最终确认的草稿。
- 对话正文以明文应用数据保存在本机；窗口正文、选中文字、临时草稿和采集结果正文不进入历史记录。
- 历史对话之间彼此隔离，不会跨对话注入消息、摘要或其他记忆信息。
- API Key 只保存在 Windows Credential Manager；`Authorization`、Cookie、Token、Secret 等敏感自定义 Header 会被拒绝。
- Mock Provider 不发送网络请求。
- 当前未实现 OCR、持续截图、剪贴板读取、活动历史、语音、Agent 或电脑操作。

## 已知限制

- UI Automation 的结果取决于目标应用的辅助功能实现，可能只返回部分文字或完全不可用。
- 当前只支持 Windows；macOS Keychain、Linux Secret Service 和其他平台集成仍只有抽象边界。
- 截图和浏览器扩展尚未实现；图片上下文会按模型能力展示，但采集入口仍不可用。
- 快捷键可能被其他应用占用；注册失败不会阻止程序启动，仍可单击助手形象。
- 应用重启后不会自动恢复上次打开的对话，需要从历史抽屉手动载入。
- 429 限流错误不会自动重试，避免在用户不知情时重复请求或计费。
- “测试连接”依赖 OpenAI-Compatible 模型详情端点；未实现该端点的 Provider 仍可能正常对话，但连接测试会失败。
- 助手形象的透明区域仍属于窗口命中区域，暂不支持逐像素鼠标穿透。

## 许可证

[MIT](LICENSE)
