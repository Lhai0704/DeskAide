# DeskAide

DeskAide 是一个常驻桌面的电子 AI 助手入口。当前版本提供 Windows 透明助手形象、可拖动双窗口、OpenAI-Compatible 文字模型和可替换的 Avatar Pack。

> 只有用户主动发送时，当前会话的文字历史才会提交给所选模型。当前版本不会读取屏幕、活动窗口、剪贴板或其他应用内容。

## 当前功能

- 透明、无边框、始终置顶的 Avatar Window
- 单击形象或按 `Ctrl + Shift + Space` 打开 Assistant Window
- 多显示器工作区定位和 Avatar 位置持久化
- 可替换的静态 Avatar Pack Manifest
- OpenAI-Compatible `/v1/chat/completions` 流式与非流式响应
- 多个模型 Profile、默认模型、模型切换和连接测试
- Windows Credential Manager 中按 Profile 隔离的 API Key
- 保留本地 Mock Provider，离线时仍可运行
- 真正发送多轮文字历史，支持新建会话、停止生成和 Assistant 展开/收起
- 从 Rust 读取模型能力，并明确禁用尚不可用的上下文选项
- `PlatformIntegration`、`ContextProvider` 和 `ModelProvider` 扩展边界

## 环境要求

- Windows 10 1803 或更新版本
- Microsoft Edge WebView2 Runtime
- Microsoft C++ Build Tools（选择“使用 C++ 的桌面开发”）
- Rust stable MSVC toolchain
- Node.js 24+ 和 npm 11+

详细的 Tauri Windows 前置条件见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)。

## 开发

```powershell
npm install
npm run tauri --workspace @deskaide/desktop -- dev
```

首次启动时只显示电子管家形象。拖动可改变位置；单击或按 `Ctrl + Shift + Space` 打开问答面板。

展开 Assistant 后打开“模型设置”，新建 OpenAI-Compatible Profile。Base URL 可以填写 Provider 根路径（例如 `https://api.longcat.chat/openai`）或以 `/v1` 结尾的 API 地址；API Key 只写入系统凭据库。保存后再由用户主动点击“测试连接”。

LongCat 示例配置：

```text
Base URL: https://api.longcat.chat/openai
Model ID: LongCat-2.0
上下文长度: 1048576
最大输出 Token: 131072
```

## 检查与构建

```powershell
npm run format:check
npm run lint
npm run check
npm run test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run tauri --workspace @deskaide/desktop -- build --debug --no-bundle
```

## 项目结构

```text
apps/desktop/              Svelte 前端与 Tauri Windows 应用
crates/assistant-core/     共享请求、消息、上下文和事件类型
crates/ai-provider/        Mock 与 OpenAI-Compatible ModelProvider
crates/context-core/       ContextProvider 与 PlatformIntegration
crates/platform-windows/   Windows 平台能力边界
docs/                      架构、资源格式和已知限制
```

## 平台策略

当前只实现和验证 Windows。业务层不会直接调用 Windows API；未来可以新增 `platform-macos` 或 `platform-linux` crate，并在桌面组合入口注入对应的 `PlatformIntegration`，无需修改模型和上下文接口。

## 隐私边界

- 当前阶段完全不采集电脑上下文。
- API Key 不进入 Tauri Store，后端也不通过 IPC 返回明文；前端只能读取“已设置/未设置”。
- `Authorization`、Cookie、Token、Secret 等敏感自定义 Header 会被拒绝。
- 未实现 OCR、持续截图、活动历史、语音、Agent 或电脑操作。
- Mock Provider 不发送网络请求。
- 未来只有用户明确选择上下文时才允许采集对应内容。

## 已知限制

- 选中文字、窗口文字、截图和浏览器扩展尚未实现，调用平台接口会返回明确的 `Unsupported`。
- 快捷键可能被其他应用占用；注册失败不会阻止应用启动，仍可单击 Avatar。
- 会话和消息只保存在当前 Assistant 窗口内存中，重启后不会恢复。
- 本阶段只有 Windows Credential Manager 具体实现；macOS Keychain 和 Linux Secret Service 只保留后端抽象。
- 429 会显示明确的限流错误，但当前不自动重试，避免在用户不知情时重复请求或计费。
- “测试连接”使用 OpenAI-Compatible 模型详情端点；不实现该标准端点的 Provider 可能无法使用连接测试，但不影响其对话接口。
- 上下文选择器已按模型能力显示禁用原因，但采集功能尚未接入，不能勾选任何上下文。
- Avatar 透明区域仍属于窗口命中区域；逐像素鼠标穿透不在第一阶段范围内。

## 许可证

[MIT](LICENSE)
