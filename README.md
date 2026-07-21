# DeskAide

DeskAide 是一个常驻桌面的电子 AI 助手入口。第一阶段提供 Windows 透明助手形象、可拖动双窗口、本地 Mock 流式响应和可替换的 Avatar Pack。

> 当前没有接入真实模型，也不会读取屏幕、活动窗口、剪贴板或其他应用内容。

## 当前功能

- 透明、无边框、始终置顶的 Avatar Window
- 单击形象或按 `Ctrl + Shift + Space` 打开 Assistant Window
- 多显示器工作区定位和 Avatar 位置持久化
- 可替换的静态 Avatar Pack Manifest
- 通过 `ModelProvider` 运行的本地 Mock 流式响应
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
crates/ai-provider/        ModelProvider 与 MockProvider
crates/context-core/       ContextProvider 与 PlatformIntegration
crates/platform-windows/   Windows 平台能力边界
docs/                      架构、资源格式和已知限制
```

## 平台策略

当前只实现和验证 Windows。业务层不会直接调用 Windows API；未来可以新增 `platform-macos` 或 `platform-linux` crate，并在桌面组合入口注入对应的 `PlatformIntegration`，无需修改模型和上下文接口。

## 隐私边界

- 第一阶段完全不采集电脑上下文。
- 未实现 OCR、持续截图、活动历史、语音、Agent 或电脑操作。
- Mock Provider 不发送网络请求。
- 未来只有用户明确选择上下文时才允许采集对应内容。

## 已知限制

- 选中文字、窗口文字、截图和浏览器扩展尚未实现，调用平台接口会返回明确的 `Unsupported`。
- 快捷键可能被其他应用占用；注册失败不会阻止应用启动，仍可单击 Avatar。
- Assistant 目前只支持单次 Mock 请求，不包含停止生成、会话持久化和真实模型配置。
- Avatar 透明区域仍属于窗口命中区域；逐像素鼠标穿透不在第一阶段范围内。

## 许可证

[MIT](LICENSE)

