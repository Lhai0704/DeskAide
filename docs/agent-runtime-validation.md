# Assistant Runtime 与 MCP 验证记录

验证日期：2026-09-21。范围为 Phase 1 + Phase 2 的 Runtime、上下文、hooks、工具循环、批准、结构化历史、MCP stdio 与现有语音入口迁移。以下是 Windows 本地检查结果；GitHub Actions 状态以对应 PR 的检查页面为准。

## 自动化结果

| 检查                                                    | 结果                                          |
| ------------------------------------------------------- | --------------------------------------------- |
| `npm run format:check`                                  | 通过                                          |
| `npm run lint`                                          | 通过                                          |
| `npm run check`                                         | 0 错误、0 警告                                |
| `npm run test`                                          | 13 个测试文件、50 个测试通过                  |
| `npm run build`                                         | 通过                                          |
| `cargo fmt --all -- --check`                            | 通过                                          |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过                                          |
| `cargo test --workspace`                                | 81 次测试执行通过，包含集成路径复用的历史测试 |
| `npm run build:debug`                                   | 通过，生成独立 Debug EXE                      |
| `git diff --check`                                      | 通过                                          |
| `npm audit --audit-level=moderate`                      | 未发现已知漏洞                                |

先构建前端资源，再执行 Rust 检查及桌面构建。Debug 产物位于 `target/debug/deskaide.exe`，不提交构建产物。现有 Windows CI 已覆盖格式、lint、类型检查、测试、前端构建和桌面 Debug 构建。

## 覆盖范围

- Runtime：文本/流式完成、失败、取消、旧 turn 隔离、工具循环和轮数限制。
- Context Registry / hooks：来源隔离、快照、替换/追加、容量限制、清理、订阅顺序及错误策略。
- 工具与批准：注册、参数校验、逐次允许/拒绝、调用失败、超时和取消；界面组件测试使用模拟 Tauri IPC。
- OpenAI-Compatible：本地 JSON/SSE fixtures 覆盖工具调用重建、多调用、混合文本、畸形响应及能力开关。
- 历史：临时目录内验证 v1 迁移、原文件保留、结构化重载、敏感调用省略及显式保存；不读取真实用户聊天记录。
- MCP：Rust fixture 子进程验证握手、分页发现、调用、退出、超时、取消、帧限制、stderr 隔离与进程树清理。
- 集成路径：本地模型 HTTP fixture → Runtime → 批准 → MCP stdio fixture → 工具结果 → 后续模型回答 → 历史落盘及重载后继续对话。
- TTS：验证 Assistant 文字生命周期接入、停止/切换、历史不朗读与迟到音频隔离；保留原有语音服务实现。

## 已知提示与验证边界

本机 Rust 1.97.1 / MSVC 链接阶段输出创建导入库的信息提示；测试和构建正常退出。Tauri 提示应用标识 `com.deskaide.app` 的 `.app` 后缀不推荐用于 macOS；为保持现有 Windows 应用身份和凭据兼容，本次未修改标识。

本次没有使用真实模型 API Key、第三方 MCP 服务或 GPU 进行人工听音验证，也不声称覆盖所有 Windows 应用的 UI Automation 行为。此前的实机语音测量见 [语音播报验证记录](voice-playback-validation.md)，不等同于本次架构迁移后的实机复测。

未实现 memory、computer use、语音输入、remote MCP、自定义 secret env 或插件市场。MCP 程序以当前用户权限运行，逐次批准不是操作系统沙箱；配置和隐私限制见 [MCP 说明](mcp.md) 与 [安全政策](../SECURITY.md)。
