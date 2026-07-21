# 第一阶段架构

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

Tauri 在启动时创建 `avatar` 和 `assistant` 两个窗口。Assistant 默认隐藏。单击 Avatar 和全局快捷键最终调用同一个 Rust `toggle_assistant` 命令。

Rust 根据 Avatar 所在显示器的物理工作区计算 Assistant 位置，顺序为右、左、下、上，最后执行边界限制。Avatar 移动时 Assistant 跟随；停止移动 220ms 后将位置写入 Tauri Store。

## Mock 数据流

```text
Assistant 输入
  → submit_mock_request
  → Arc<dyn ModelProvider>
  → Tokio mpsc ResponseEvent
  → Tauri model-response 事件
  → Svelte 流式渲染
```

核心层的事件发送器不依赖 Tauri，因此真实 Provider、测试和其他前端都可复用同一接口。

## 平台扩展

当前只有 `platform-windows`，且上下文方法明确返回 `Unsupported`。未来新增平台时创建独立 crate，实现 `PlatformIntegration`，并在各自构建目标的组合入口注入。

