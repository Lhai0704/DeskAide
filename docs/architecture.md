# 当前架构

## 依赖方向

```text
assistant-core
    ↑             ↑
ai-provider   context-core
                    ↑
             platform-windows

desktop-tauri → 上述所有 crate
desktop-svelte → 仅通过 Tauri IPC 和事件访问后端
```

`assistant-core` 不依赖 UI、Tauri 或平台 API。`ModelProvider` 和 `PlatformIntegration` 作为 trait object 注入桌面状态，后续替换实现不需要修改 Assistant UI。

## 窗口协调

Tauri 在启动时创建 `avatar` 和 `assistant` 两个窗口。Assistant 默认隐藏，助手形象窗口不可获取焦点。单击助手形象和全局快捷键最终调用同一个异步 Rust `toggle_assistant` 命令。

打开 Assistant 前，Rust 先记录外部前台窗口元数据和 UI Automation 焦点元素，然后再显示并聚焦 Assistant。Rust 根据助手形象所在显示器的物理工作区计算 Assistant 位置，顺序为右、左、下、上，最后执行边界限制。助手形象移动时 Assistant 跟随；停止移动 220ms 后将位置写入 Tauri Store。

## 模型请求数据流

```text
Assistant 输入
  → submit_model_request（当前所选对话的完整文字历史）
  → 按用户本次选择加入已确认的窗口文字草稿，并按需采集激活前选中文字
  → 按模型上下文窗口限制长度
  → 读取当前 ModelProfile
  → 按 Profile 从 CredentialStore 取得 API Key
  → 构造 MockProvider 或 OpenAiCompatibleProvider
  → Tokio mpsc ResponseEvent
  → Tauri model-response 事件
  → Svelte 流式渲染
```

核心层的事件发送器不依赖 Tauri，因此 Provider、测试和其他前端都可复用同一接口。真实 HTTP 请求由可取消的 Tauri 异步任务持有；停止生成会 abort 任务并释放响应流。

`OpenAiCompatibleProvider` 把以 `/v1` 结尾和 Provider 根路径两类 Base URL 统一为 `/v1/chat/completions`。请求支持 system/user/assistant、多轮文字、temperature 和 max_tokens。用户授权的桌面文字以明确标记的不可信引用区块加入当前用户消息，不进入后续会话历史。响应层分别处理标准 JSON 与增量 SSE，并把 HTTP、网络、超时、格式和流中断映射为稳定错误代码。

## 语音播报数据流

```text
model-response → SpeechText 增量过滤与分句
  → SpeechController（会话快照、串行片段队列、15 秒播放缓冲门槛）
  → speak_segment（Rust / 本地 HTTP）
  → fast-qwen3-tts /generate/stream（persist=false）
  → NDJSON Float32 PCM → Tauri Channel → SpeechPlayer / Web Audio
```

语音设置独立存为 Tauri Store `settings.json` 的 `speech` 项，包含开关、音量、参考声音文件标识、模型及本地项目目录。`get_speech_settings`、`save_speech_settings` 管理设置，`check_speech_service`、`speech_references` 负责服务探测和素材列表。前端不直接访问 HTTP，不持久化合成音频。

每个播报会话固定声音和模型配置；所有 Channel 消息携带会话、文字请求与片段 ID。停止朗读立即关闭 AudioContext 并丢弃迟到消息，再通过 `cancel_speech` 取消后端任务。服务端在加载结束或音频分块处检查取消，Rust 在旧 GPU 工作结束前保持队列串行。隐藏窗口不销毁播报会话；新问题、切换对话或停止生成会取消它。

Rust 合并并发启动请求、验证 TTS 服务身份和临时播报/取消能力，使用项目内 Python 与离线环境启动服务。已存在的兼容服务直接复用。自启服务接收 `DESKAIDE_PARENT_PID`，持有 DeskAide 的 Windows 进程句柄，父进程退出后结束运行；正常退出回调也清理自有进程树。手动启动的服务不受影响。

TTS 暂不可用、忙碌、超时或音频格式错误只会结束本轮语音，不改变文字生成和历史保存。音频与文本均不写入 TTS 作品库或正文日志；本地参考素材及其预处理缓存仍由 TTS 管理。

## Profile 与凭据边界

`ProfileCollection` 管理内置 Mock 和用户模型，普通字段以 `SavedProfiles` 写入 Tauri Store。默认 Profile ID 与 Profile 一起保存；启动时即使 Store 为空或损坏，也会恢复 Mock Profile。

`CredentialStore` 隔离平台凭据实现。Windows 使用 Credential Manager，标识为 `service=com.deskaide.app`、`account=model-profile:{profile_id}`。API Key 只在后端创建 HTTP Provider 时短暂取出，不参与 Profile 序列化、Debug 或错误输出。前端模型视图只有 `hasApiKey` 布尔值。

## Assistant 交互壳层

Rust 通过 `get_assistant_bootstrap` 暴露当前模型 Profile 和 `ModelCapabilities`，通过 `assistant-shown` 暴露本次外部目标。Svelte 只在目标存在时启用选中文字和窗口文字，并展示每项的成功、不可用、失败或截断结果；网页和图片项继续显示明确的未实现原因。

每次请求都注册唯一请求 ID 和可取消任务句柄。`stop_generation` 仅取消匹配的活动请求，并发送 `Cancelled` 事件；前端 reducer 会忽略其他请求的迟到事件。每次请求会按原角色顺序发送当前所选对话的完整文字历史。

有用户消息的会话通过独立的 Tauri Store 文件 `conversation-history.json` 持久化。存储数据带版本号，包含标题、最近使用的模型 Profile、可见消息和时间戳；列表按最近更新时间排序。应用启动时创建空白会话，只有用户主动从历史抽屉选择记录后才恢复旧对话。流式响应仅在完成、失败或停止时写入，不按增量频繁落盘。

历史对话不是记忆层：窗口正文、选中文字、上下文草稿和采集结果仍只参与当次请求，不写入历史，也不会在不同对话之间自动注入。

Assistant 支持 420×460 的紧凑模式和最大 720×720 的展开模式。Rust 按当前 DPI 转换尺寸、限制到助手形象所在显示器工作区，并复用窗口定位算法重新靠近助手形象。

## 平台扩展

当前只有 `platform-windows` 实现可见窗口枚举、外部窗口追踪、选中文字和指定窗口的可访问文字。UI Automation 在专用 COM 工作线程执行并为单次采集设置三秒超时；截图方法仍明确返回 `Unsupported`。未来新增平台时创建独立 crate，实现 `PlatformIntegration`，并在各自构建目标的组合入口注入。
