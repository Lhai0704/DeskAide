# 当前架构

## 依赖方向

```text
assistant-core：共享 DTO、AssistantEvent、Session / TranscriptMessage
    ↑
ai-provider / context-core / tool-core：领域接口与实现
    ↑
assistant-runtime：上下文组合、hooks、turn 状态机和工具循环

mcp-client → tool-core：stdio 工具适配与进程生命周期
platform-windows → context-core：Windows 原生采集
Tauri → 上述 crate：组合入口、IPC、持久化与桌面生命周期
Svelte → Tauri IPC / assistant-event：UI 与语音消费
```

核心 crate 不依赖 Tauri。`assistant.rs` 是组合入口与聊天/MCP IPC；`providers.rs` 创建 Provider 并取得凭据；`context_commands.rs` 保留桌面预览/编辑适配；窗口协调和快捷键仍使用原实现。

## Runtime 与事件

UI 提交 `submit_turn`：conversation ID、全新 turn UUID、预期 revision、问题和明确添加的草稿。后端 `SessionRepository` 是 transcript 的唯一来源，UI 不再从气泡拼模型历史。`AssistantRuntime::start` 在异步上下文采集、MCP 发现和模型请求之前注册身份；新 turn 取消并等待上一 turn 完成清理。

`AssistantEvent` 为版本化 tagged enum，使用 camelCase，携带 conversationId、turnId、单调递增 sequence。事件覆盖 turn 起止、上下文结果、每步 assistant message 边界、文字/推理增量、工具提议/批准/执行结果和 usage。reasoning 只作为实时事件，不入历史或语音。

Provider 仅输出 `ProviderEvent` 增量和最终 `ModelResponse`；模型完成一次请求不等于整个 turn 完成。核心事件通道容量 256，UI 出口发送最多等待一秒；权威 turn snapshot 保留最近 16 轮，有界响应内容。Svelte 按 conversation/turn/sequence 过滤迟到或重复事件，终态后不接受增量；WebView 重新订阅时通过 `get_active_turn_snapshot` 找回运行中的请求，序号缺口与丢失终态通过 `get_turn_snapshot` 恢复，恢复文字不触发朗读。

取消 token 覆盖上下文 future、HTTP、hooks、批准及工具执行。唯一终态在最终持久化后发出；新旧任务不能互相清空句柄。失败的持久化明确报错，不将未保存的结果宣称已保存。取消外部工具不是撤销操作：未确认结果保留 unknown/interrupted，不自动重试。

## Context Registry 与 hooks

`context-core` 复用平台采集、预算截断和错误映射，新增 `ContextRegistry`：source key、typed ContextPayload、replace-self / append-self、克隆隔离的 snapshot、clear / reset。每轮创建独立 registry，本次唯一 scope 是 CurrentTurn。最多 128 个来源、每来源 128 项、单项约 250 KiB、合计 1 MiB；操作历史最多 128 条，仅含操作、数量、字节和序号，不含正文或窗口标题。

桌面文字渲染从 Provider 移至 runtime prompt composition，仍明确标记为不可信引用。预算和 UI Automation 三秒超时保持；一轮中的工具循环复用同一份内存引用，不重新采集。原始桌面内容、草稿和推理不进入 SessionRepository。

`HookRegistry` 按注册顺序执行 typed phase + HookData：组合前后、模型请求前、文字/推理增量、工具提议/开始/结束和 turn 终态。提供只读 prompt/messages/request/call/result 数据；订阅 handle 显式 unsubscribe 或 Drop 清理，registry 可 clear，最多 128 listeners。分发使用注册快照，执行回调不持锁。单回调最多 200ms，观测失败/超时仅记录 ID、阶段和固定错误类别并在本轮停用；仅请求前显式 guard 可阻止发送。权限判断和持久化不放在可失败的观测 hook 中。

## 工具循环与权限

`tool-core` 注册稳定 ID、模型调用名、展示名、JSON Schema、来源、风险及定义 revision；重复 ID/name 报错，支持按 MCP owner 清理。Schema 本地验证，不加载网络/文件 `$ref`；不支持外部引用或 `$id` 基址。参数完整解析并验证后才可能执行。工具结果统一为结构化 ToolResult / ToolError。

风险枚举为 ReadOnly / UserData / Mutating / ExternalSideEffect / Unknown。只有内部实现明确声明的 ReadOnly 自动执行，所有 MCP（包括自报只读）要求逐次批准。批准绑定 turn、call、定义及参数快照；无永久权限。变更/删除 server 会撤销工具，使等待失效；执行前再次校验定义。拒绝和超时作为工具结果交回模型。

同一步的多工具按返回顺序串行批准和执行，全部结果配对后再次请求模型。集中默认限制：8 次模型请求、单步 16 个/整轮 32 个调用、工具 60 秒、批准 5 分钟、整轮 15 分钟，参数 64 KiB、结果 128 KiB。超过限制不裁剪参数后执行；工具失败通常仍可由模型继续处理。

`OpenAiCompatibleProvider` 支持 JSON/SSE、多个交错 tool-call delta、混合文字、usage、assistant tool_calls 和 role:tool 配对结果。流结束才提交完整调用；截断流不执行。Profile 的 supportsTools 默认 false；关闭时不发送 tools/tool_choice，并把旧工具记录投影为明确标记的参考文字，存储仍保持结构化。不自动重发失败请求。

## MCP stdio

`mcp-client` 使用官方 `rmcp = 3.4.0` 的 client/async-rw，负责 initialize、分页 tools/list、tools/call、超时、取消、退出检测和清理；无 HTTP/SSE/OAuth/server features。模型别名由 server ID 与原始工具名生成稳定哈希，执行器保存精确原名映射。

程序以 executable + args 启动，stdout 是有上限的协议通道（单帧 1 MiB），stderr 独立排空且不记录正文。Windows Job Object、隐藏控制台和 KillOnDrop 管理自有进程树；正常退出先关闭 stdio，限时后终止子树。SDK 不获得 sampling、elicitation 或桌面控制能力。

设置仅加载不启动；支持工具的 turn 按需连接 enabled servers。并发启动在每 server 的锁内合并，批准和执行持有租约，空闲 5 分钟退出；崩溃标记 failed，由用户重连，无无限重启。设置页测试创建独立临时 manager 并清理。详细安全与配置限制见 [MCP 说明](mcp.md)。

## 结构化历史 v2

`conversation_history.rs` 使用串行锁、revision CAS、临时文件写入 + sync + 原子替换保存 `conversation-history-v2.json`，总上限 64 MiB；满时报告错误，不自动删除记录。版本、消息 ID 和 tool-call/result 配对均验证。用户消息、调用前 pending 结果、工具完成和 turn 终态等语义边界保存，不按 token 落盘。

首次读取旧 `conversation-history.json` v1（包括 Store 的 history 包装）后转换纯文字消息，验证成功写独立 v2，原 v1 保留。存在 v2 时绝不回退 v1，避免已删除对话复活。未知版本/损坏/写入失败保留原文件并报错。重启清理 activeTurn，未完成工具结果保持 interrupted/unknown，不重执行。

Transcript 包含 user/assistant/tool 消息、call ID/参数、结果、工具来源与风险、turnStatus、note/omitted；UI 只投影可见消息。含临时桌面上下文时先持久化省略占位，再等待批准；单独勾选“保存参数和结果”才允许本次原文落盘。未勾选仍保留调用关系、来源及状态，下一轮模型能看到明确的 unavailable 标记。没有桌面上下文的调用按通常规则保存。助手回答主动引用的文字仍随回答保存，和原有聊天行为一致。

历史保留标题、Profile、时间戳、排序、重命名和删除，启动仍默认空白会话。这不是 Memory；没有自动提取、跨会话注入或后台上下文采集。

## 窗口协调

### Avatar presentation

`avatar/behavior` 将权威 turn phase 与真实播放信号转换成 idle/activated/thinking/responding/speaking/error；renderer 不读取 conversation 或 reasoning。`TurnSnapshot.phase` 表示 preparing/generating/responding/tool/approval/terminal，Runtime 不依赖 avatar。avatar WebView 串行读取 Tauri presentation snapshot，以 revision 拒绝迟到结果；快照读取天然补偿漏事件，不转发聊天正文。语音 publisher 使用 WebView epoch + sequence 隔离旧请求，信号只保存在内存并有过期保护。

`SpeechPlayer` 在 GainNode 后通过 AnalyserNode 计算平滑 RMS，用 AudioContext 排程区间判断实际播放。TTS 只发布通用播放信号；隐藏 Assistant 不销毁音频，关闭或新会话立即清零。

Live2D renderer 通过独立本地 SDK adapter 使用官方 Cubism Web Framework 5-r.5/WebGL2。SDK/Core 不进入源码仓库，只有选择 Live2D 才加载。manifest v3 保留 v1/v2；catalog 合并 bundled pack 与受限本地 pack。原生适配只提供窗口 resize、受限文件资源和 GetCursorPos 当前坐标采样。形象不增加 MCP/LLM tool。

VRM renderer 使用随应用打包的 three.js 与 `@pixiv/three-vrm`，只在选择 VRM 形象后创建 WebGL 场景。manifest v4 的模型是包内 `.vrm`，可选 `.vrma` 对应语义状态。取景固定为上半身，注视、眨眼、呼吸和口型都由同一份 presentation 快照驱动。VRM 0 会转到与 VRM 1 相同的朝向，相机在模型正前方。

形象尺寸来自 manifest，保留底部中心并限制工作区，最大 640×900 逻辑像素；DPI 变化重新协调。鼠标超过 5 个逻辑像素才启动原生拖动。Live2D 按可见网格、VRM 按骨骼胶囊发布同一张低分辨率命中遮罩，遮罩外的点击落到桌面；静态和视频形象仍是整窗命中。具体资源生命周期、许可与测试边界见 [Live2D](live2d.md) 和 [形象资源包](avatar-pack-format.md)。

Tauri 在启动时创建 `avatar`、`assistant` 和 `context-editor` 三个窗口。Assistant 和上下文编辑器默认隐藏，助手形象窗口不可获取焦点。单击助手形象调用异步 Rust `toggle_assistant` 命令切换面板；全局快捷键在面板隐藏时显示面板，已显示时只聚焦，不再次隐藏。

打开 Assistant 前，Rust 先记录外部前台窗口元数据和 UI Automation 焦点元素，然后再显示并聚焦 Assistant。Rust 根据助手形象所在显示器的物理工作区计算 Assistant 位置，顺序为右、左、下、上，最后执行边界限制。助手形象移动时 Assistant 跟随；停止移动 220ms 后将位置写入 Tauri Store。

## 全局快捷键

`shortcuts.rs` 管理激活入口，设置页通过 Tauri IPC 读取与保存 `shortcut` 和 `copilotEnabled`，存储在 `settings.json` 的 `shortcuts` 键中。备用组合键通过 Tauri global-shortcut 插件注册；格式错误或注册冲突时返回错误，保留已有配置。

标准 Copilot 键的 Win + Shift + F23 由专用 Windows 消息线程上的低级键盘钩子处理。钩子仅抑制匹配的 F23 按下与抬起，通过通道通知激活逻辑；长按去重，不采集或保存键入文字。关闭开关或退出程序后恢复系统行为，不更改系统按键映射。启动注册或监听失败会显示在快捷键设置页，点击入口仍可使用。

## 语音播报数据流

```text
AssistantEvent 文字生命周期 → SpeechText 增量过滤与分句
  → SpeechController（会话快照、串行片段队列、15 秒播放缓冲门槛）
  → speak_segment（Rust / 本地 HTTP）
  → fast-qwen3-tts /generate/stream（persist=false）
  → NDJSON Float32 PCM → Tauri Channel → SpeechPlayer / Web Audio
```

语音设置独立存为 Tauri Store `settings.json` 的 `speech` 项，包含开关、音量、参考声音文件标识、模型及本地项目目录。`get_speech_settings`、`save_speech_settings` 管理设置，`check_speech_service`、`speech_references` 负责服务探测和素材列表。前端不直接访问 HTTP，不持久化合成音频。

每个 turn 固定播报会话、声音和模型配置；messageCompleted 刷新分句余量，turnCompleted 结束本轮，不朗读推理、工具参数/结果或批准卡片。待合成队列最多 128 段 / 64,000 字符，超限仅结束语音。所有 Channel 消息携带会话、文字请求与片段 ID。停止朗读立即关闭 AudioContext 并丢弃迟到消息，再通过 `cancel_speech` 取消后端任务。服务端在加载结束或音频分块处检查取消，Rust 在旧 GPU 工作结束前保持队列串行。隐藏窗口不销毁播报会话；新问题、切换对话或停止生成会取消它。

Rust 合并并发启动请求、验证 TTS 服务身份和临时播报/取消能力，使用项目内 Python 与离线环境启动服务。已存在的兼容服务直接复用。自启服务接收 `DESKAIDE_PARENT_PID`，持有 DeskAide 的 Windows 进程句柄，父进程退出后结束运行；正常退出回调也清理自有进程树。手动启动的服务不受影响。

TTS 暂不可用、忙碌、超时或音频格式错误只会结束本轮语音，不改变文字生成和历史保存。音频与文本均不写入 TTS 作品库或正文日志；本地参考素材及其预处理缓存仍由 TTS 管理。

## Profile 与凭据边界

`ProfileCollection` 管理内置 Mock 和用户模型，普通字段以 `SavedProfiles` 写入 Tauri Store。默认 Profile ID 与 Profile 一起保存；启动时即使 Store 为空或损坏，也会恢复 Mock Profile。

`CredentialStore` 隔离平台凭据实现。Windows 使用 Credential Manager，标识为 `service=com.deskaide.app`、`account=model-profile:{profile_id}`。API Key 只在后端创建 HTTP Provider 时短暂取出，不参与 Profile 序列化、Debug 或错误输出。前端模型视图只有 `hasApiKey` 布尔值。

## Assistant 交互壳层

Rust 通过 `get_assistant_bootstrap` 暴露当前模型 Profile 和 `ModelCapabilities`，通过 `assistant-shown` 暴露本次外部目标。Svelte 只在目标存在时启用选中文字和窗口文字，并展示每项的成功、不可用、失败或截断结果；网页和图片项继续显示明确的未实现原因。

Assistant 支持 420×460 的紧凑模式和最大 720×720 的展开模式。Rust 按当前 DPI 转换尺寸、限制到助手形象所在显示器工作区，并复用窗口定位算法重新靠近助手形象。

## 平台扩展

当前只有 `platform-windows` 实现可见窗口枚举、外部窗口追踪、选中文字和指定窗口的可访问文字。UI Automation 在专用 COM 工作线程执行并为单次采集设置三秒超时；截图方法仍明确返回 `Unsupported`。未来新增平台时创建独立 crate，实现 `PlatformIntegration`，并在各自构建目标的组合入口注入。

## 明确不实现的能力

本次未实现 memory、computer use、鼠标键盘操作、Shell Agent、语音输入/麦克风/ASR/VAD、VRM、OCR、浏览器或编辑器扩展、remote MCP、云端服务、WebSocket 插件运行时或插件市场。Live2D 仅属于本地表现层。stdio 程序本身可具有外部访问能力，工具批准不构成 OS 沙箱。
