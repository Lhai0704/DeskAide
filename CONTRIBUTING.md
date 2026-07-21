# 参与贡献

感谢参与 DeskAide。当前项目以 Windows 为唯一运行目标，同时要求业务层保持平台无关。

## 开发流程

1. 创建功能分支并保持改动聚焦。
2. Windows API 只允许出现在 `crates/platform-windows` 或桌面组合入口。
3. 不提交 API Key、凭据、个人日志或构建产物。
4. 提交前运行 README 中列出的全部检查。
5. 行为变更应附带单元测试和必要的文档更新。

## 范围约束

当前阶段不接受 OCR、后台监控、笔记集成、MCP、电脑操作或自主 Agent 实现。新增上下文能力必须由用户明确触发，并通过 `PlatformIntegration` 或 `ContextProvider` 暴露。

