# DeskAide

DeskAide 是一个常驻 Windows 桌面的电子 AI 助手入口。它以可拖动的透明助手形象驻留在桌面，通过独立的 Assistant 面板连接 OpenAI-Compatible 模型，支持经批准的本地 MCP 工具，并允许用户把选中文字、剪贴板或多个外部窗口中可访问的文字作为本轮提问的上下文。

当前项目基于 Tauri 2、Svelte 5 和 Rust，仍处于早期开发阶段，仅实现并验证 Windows。

> DeskAide 不会持续读取屏幕或监控其他应用。只有用户点击预览或添加相应来源时，才会读取文字；发送前可以查看、编辑和移除草稿，发送后不自动沿用到下一轮。

## 功能概览

### 桌面助手与外观

- 透明、无边框、始终置顶的助手形象窗口；支持拖动、多显示器工作区定位和位置持久化。
- 单击助手形象，或按 Copilot 键 / `Ctrl + Shift + Space` 激活 Assistant 面板；快捷键可在“设置 → 快捷键”中修改。
- Assistant 面板支持紧凑/展开、临时置顶、失焦隐藏，以及跟随助手形象重新定位。
- 支持浅色与深色主题，选择会保存在本机并在下次启动时恢复。
- 默认使用机器人静态形象，支持单击激活和拖动。
- 助手形象由 Manifest 驱动，保留 static/video，并支持本地 Live2D v3 pack：窗口外鼠标注视、idle/眨眼、状态动作、点击反馈和真实播放音量驱动嘴型。
- 支持本地 VRM v4 pack：`.vrm` 模型、可选 `.vrma` 状态动作、鼠标注视、眨眼、呼吸，以及播报音量驱动的口型。角色以外的空白可以点到桌面。
- Live2D SDK/Core 和角色资源需按[本地准备说明](docs/live2d.md)提供；VRM 运行时已随应用打包，模型文件放在同一本地形象目录。默认仍使用仓库内的机器人形象。

### 对话与模型

- 支持 OpenAI-Compatible `/v1/chat/completions` 流式与非流式响应。
- 真正发送当前对话的多轮文字历史，支持新建对话和停止生成。
- 有用户消息的对话自动保存在本机；历史抽屉支持继续对话、重命名和删除。
- 每条历史记录保存其模型 Profile；载入时会尝试恢复原模型，但应用启动后仍默认进入空白新对话。
- 支持多个模型 Profile、默认模型、对话中切换模型和手动连接测试。
- Google 官方兼容端点的 `gemini-3.5-flash` 提供“优先快速回应”（默认开启），使用 `reasoning_effort: minimal` 减少简单聊天的思考等待；关闭后使用服务默认思考级别。该模型保留服务默认采样参数，其他模型不受影响。流式输出不能消除服务开始输出前的等待，实际延迟仍取决于服务和网络。
- API Key 按 Profile 隔离保存在 Windows Credential Manager，不进入普通配置文件或前端 IPC 响应。
- 内置不发送网络请求的 Mock Provider，未配置真实模型时也可离线体验和开发；Mock 返回收悉提示，不回显可能含临时上下文的完整输入。
- Rust 后端向前端提供模型能力和上下文窗口大小，尚不可用的上下文选项会显示明确原因。

### Agent 与本地工具

- 独立 Rust AssistantRuntime 管理 turn、取消、上下文组合和多步工具循环；Provider 只负责模型 API。
- 模型设置中的“支持工具调用”默认关闭；确认服务支持标准 OpenAI tool calling 后再开启。旧 Profile 与 Mock 保持普通聊天。
- 设置 → MCP 可添加、编辑、禁用、删除、测试和重连 stdio server，默认不启动任何进程；启用的 server 在支持工具的聊天中按需连接。
- 外部 MCP 工具逐次展示工具名、来源及完整可展开参数，点击 **Allow once / Deny**。拒绝、失败和超时会作为结果交回模型；停止生成、新问题或切换会话取消旧等待。
- 多调用按顺序执行；每轮最多 8 次模型请求、32 次工具调用。工具错误不会使普通聊天无法使用。
- 结构化 v2 历史保留模型调用和工具结果。含临时桌面上下文时，参数和结果默认不保存；只有批准卡片中另行勾选才保存本次调用。
- 本地 MCP 程序以当前用户权限运行，逐次工具批准不是操作系统沙箱。只配置可信程序；不自动安装 server，不支持自定义环境变量或通过参数保存密钥。

配置示例、限制和清理规则见 [MCP stdio 使用说明](docs/mcp.md)。

本次 Runtime 与 MCP 升级的自动化覆盖、构建结果和未验证范围见 [架构升级验证记录](docs/agent-runtime-validation.md)。

### 可编辑的桌面上下文

- 记录 Assistant 激活前的外部窗口；用户点击预览或添加后才读取选区或剪贴板，生成可编辑草稿。
- 可以枚举当前可见的外部顶层窗口，任意多选并分别采集可访问文字。
- 每个窗口上下文以摘要卡片展示；单击后可在独立编辑窗口中预览、修改将要发送的完整草稿。
- 通过 Windows UI Automation 尽力获取选中文字或指定窗口公开的可访问文字，单次采集设有 3 秒超时。
- 发送前按当前模型的上下文预算截断文字；某项采集失败不会阻止普通提问。
- 窗口文字、选中文字和临时草稿只参与当前 turn（包括本轮工具循环），不会自动写入历史或沿用到下一轮。模型回复主动引用的内容仍会随助手回复保存。

### 实时语音播报

- 设置 → **语音播报**，点击“测试连接 / 刷新声音”，选择已有参考声音，开启自动朗读并保存。默认关闭，语音模型默认 0.6B，可切换 1.7B。
- 默认使用 `D:\Projects\fast-qwen3-tts` 的隔离环境与本地 `127.0.0.1:7860` 服务。首次连接或播报按需启动，不自动安装依赖或下载模型；首次加载模型会有额外等待。
- 需要更新后的 [本地 fast-qwen3-tts 服务](https://github.com/Lhai0704/faster-qwen3-tts)，其 `/status` 必须声明 `temporary_stream` 和 `cancel_task` 能力。先在 TTS 工作台准备参考录音，再到 DeskAide 选择；项目安装位置不同时可修改设置中的项目目录。
- 助手逐句提交文字，音频分块到达即播放；过滤代码块、Markdown 标记和裸链接。非流式文字回复在完成后开始朗读。
- “停止朗读”只停止声音；“停止生成”、新问题和切换对话会打断旧播报。隐藏面板仍继续播放，历史对话不会自动朗读。
- 音量保存后即时生效，声音和模型从下一轮生效。试听使用当前表单选项，不需要开启自动朗读。
- 语音只在内存中播放，不向 TTS 作品库写入音频或聊天正文；DeskAide 原有文字历史保存规则不变。参考音频预处理缓存仍留在 TTS 项目中。
- 退出 DeskAide 时关闭它自己启动的 TTS 进程树；已由用户启动的服务保持运行。取消 GPU 生成需等待当前加载或音频块到达检查点，声音会立即停止。
- 若服务忙碌、端口被占用、版本不兼容或合成失败，本轮播报停止并显示原因，文字对话继续。旧版 TTS 需更新接口后手动重启。

详细检查结果与实机首音测量见 [语音播报验证记录](docs/voice-playback-validation.md)。

## 环境要求

- Windows 10 1803 或更新版本
- Microsoft Edge WebView2 Runtime
- Microsoft C++ Build Tools（安装“使用 C++ 的桌面开发”工作负载）
- Rust stable MSVC toolchain（项目最低 Rust 版本为 1.88）
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
2. 点击选中文字或剪贴板的“预览”，检查后点击“添加”；也可以直接添加后编辑草稿。
3. 若要添加其他窗口，刷新窗口列表并多选目标窗口，然后生成上下文草稿。
4. 单击摘要卡片可检查和修改草稿；确认后随问题一起发送。

## 检查与构建

提交前可运行完整检查：

```powershell
npm run format:check
npm run lint
npm run check
npm run test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
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
  src/avatar/              形象行为层与 static/video/Live2D 渲染
  src/settings/            主题、形象、快捷键、语音、MCP 和模型 Profile 设置
  src-tauri/               窗口协调、IPC、凭据和本地持久化
crates/assistant-core/     共享请求、结构化 transcript、AssistantEvent 与工具 DTO
crates/assistant-runtime/  Session、hooks、上下文组合、取消和 Agent tool loop
crates/tool-core/          工具注册、Schema 校验、权限和执行接口
crates/mcp-client/         官方 rmcp stdio 客户端与自有进程管理
crates/ai-provider/        Mock 与 OpenAI-Compatible ModelProvider
crates/context-core/       Context Registry、ContextProvider 与 PlatformIntegration
crates/platform-windows/   Windows 窗口追踪与 UI Automation 能力
docs/                      架构、形象资源格式和 Windows 限制
```

更多设计说明：

- [当前架构](docs/architecture.md)
- [助手形象资源包格式（v1/v2/v3/v4）](docs/avatar-pack-format.md)
- [Live2D 本地准备、许可与验证](docs/live2d.md)
- [Windows 已知限制](docs/windows-limitations.md)

## 隐私与安全边界

- 激活 Assistant 时只为“当前选中文字”记录前一窗口的元数据和 UI Automation 元素引用，不立即读取文字。
- 用户明确添加外部窗口后才读取该窗口文字，并且发送时使用用户最终确认的草稿。
- 对话正文以明文应用数据保存在本机；窗口正文、选中文字、临时草稿和采集结果正文不进入历史记录。
- 历史对话之间彼此隔离，不会跨对话注入消息、摘要或其他记忆信息。
- API Key 只保存在 Windows Credential Manager；`Authorization`、Cookie、Token、Secret 等敏感自定义 Header 会被拒绝。
- Mock Provider 不发送网络请求。
- 当前未实现 memory、computer use、Shell Agent、语音输入、OCR、持续截图、remote MCP、云端服务或插件市场。

## 激活快捷键

“设置 → 快捷键”可开关 Copilot 键并保存备用组合键，立即生效且重启后保留。默认开启标准 Copilot 键（Win + Shift + F23），由进程内 Windows 键盘钩子拦截 F23 按下/抬起，避免同时打开系统搜索；只在 DeskAide 运行期间接管，不修改注册表。退出或关闭开关后恢复系统行为。长按只激活一次，已显示的助手获得焦点而不会被再次隐藏。

备用快捷键默认 `Control+Shift+Space`，支持如 `Alt+Space` 的组合；占用或格式错误会保留原配置并显示错误。使用前需运行 DeskAide；目前未提供开机自启设置。不同键盘固件可能发送其他键值，实际 Copilot 按键仍需在目标电脑手动验收。

## 已知限制

- UI Automation 的结果取决于目标应用的辅助功能实现，可能只返回部分文字或完全不可用。
- 当前只支持 Windows；macOS Keychain、Linux Secret Service 和其他平台集成仍只有抽象边界。
- 截图和浏览器扩展尚未实现；图片上下文会按模型能力展示，但采集入口仍不可用。
- 快捷键可能被其他应用占用；注册失败不会阻止程序启动，仍可单击助手形象。
- 应用重启后不会自动恢复上次打开的对话，需要从历史抽屉手动载入。
- 429 限流错误不会自动重试，避免在用户不知情时重复请求或计费。
- “测试连接”依赖 OpenAI-Compatible 模型详情端点；未实现该端点的 Provider 仍可能正常对话，但连接测试会失败。
- 静态和视频形象的透明区域仍属于窗口命中区域。Live2D 与 VRM 按角色轮廓放行周围点击，不是逐像素穿透。

## 许可证

[MIT](LICENSE)

MIT 仅覆盖 DeskAide 自有代码。Live2D SDK/Core 与第三方模型遵守各自许可，本仓库不包含这些资源；本地准备与发布边界见 [Live2D 文档](docs/live2d.md)。VRM 运行时使用的 three.js 与 `@pixiv/three-vrm` 为 MIT，模型文件同样不进入仓库。
