# Windows 平台状态与限制

## 已实现

- 透明、无边框、置顶窗口
- 全局快捷键
- 多显示器工作区定位
- 物理坐标位置持久化
- Windows 应用图标和 debug 构建
- Windows Credential Manager API Key 存储（每个模型 Profile 独立条目）

## 尚未实现

- 最近外部活动窗口追踪
- UI Automation 选中文字和可访问文字
- 窗口、显示器和框选截图
- 屏幕录制权限状态
- 开机启动和系统托盘
- macOS Keychain 与 Linux Secret Service 具体实现

这些方法目前由 `WindowsPlatformIntegration` 返回 `PlatformError::Unsupported`。后续实现必须位于平台 crate，不得把 Windows API 散落到模型、上下文业务层或 Svelte 组件。

## 运行差异

- `Ctrl + Shift + Space` 被占用时会输出警告，但应用继续运行。
- 多显示器布局改变后，保存位置会限制到主显示器可见工作区。
- Windows 缩放切换可能触发多次移动事件，Store 写入已做 220ms 防抖。
- 删除非活动 Profile 时会同步删除其 Credential Manager 条目；内置 Mock 与当前活动 Profile 不能删除。
