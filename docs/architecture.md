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
  → submit_model_request（完整内存会话历史）
  → 按用户本次选择采集文字上下文
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

## Profile 与凭据边界

`ProfileCollection` 管理内置 Mock 和用户模型，普通字段以 `SavedProfiles` 写入 Tauri Store。默认 Profile ID 与 Profile 一起保存；启动时即使 Store 为空或损坏，也会恢复 Mock Profile。

`CredentialStore` 隔离平台凭据实现。Windows 使用 Credential Manager，标识为 `service=com.deskaide.app`、`account=model-profile:{profile_id}`。API Key 只在后端创建 HTTP Provider 时短暂取出，不参与 Profile 序列化、Debug 或错误输出。前端模型视图只有 `hasApiKey` 布尔值。

## Assistant 交互壳层

Rust 通过 `get_assistant_bootstrap` 暴露当前模型 Profile 和 `ModelCapabilities`，通过 `assistant-shown` 暴露本次外部目标。Svelte 只在目标存在时启用选中文字和窗口文字，并展示每项的成功、不可用、失败或截断结果；网页和图片项继续显示明确的未实现原因。

每次请求都注册唯一请求 ID 和可取消任务句柄。`stop_generation` 仅取消匹配的活动请求，并发送 `Cancelled` 事件；前端 reducer 会忽略其他请求的迟到事件。会话目前只存在于 Assistant 窗口内存中，但每次请求会按原角色顺序发送当前完整文字历史。

Assistant 支持 420×460 的紧凑模式和最大 720×720 的展开模式。Rust 按当前 DPI 转换尺寸、限制到助手形象所在显示器工作区，并复用窗口定位算法重新靠近助手形象。

## 平台扩展

当前只有 `platform-windows` 实现外部窗口追踪、选中文字和可访问文字。UI Automation 在专用 COM 工作线程执行并设置三秒超时；截图方法仍明确返回 `Unsupported`。未来新增平台时创建独立 crate，实现 `PlatformIntegration`，并在各自构建目标的组合入口注入。
