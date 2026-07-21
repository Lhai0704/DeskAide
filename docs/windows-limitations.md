# Windows 平台状态与限制

## 已实现

- 透明、无边框、置顶窗口
- 全局快捷键
- 多显示器工作区定位
- 物理坐标位置持久化
- Windows 应用图标和 debug 构建
- Windows Credential Manager API Key 存储（每个模型 Profile 独立条目）
- 最近外部活动窗口快照与 DeskAide 自身进程排除
- UI Automation 焦点元素记录
- TextPattern 选中文字和可访问文档文字
- ValuePattern 与有限窗口子树回退
- 三秒采集超时和非阻塞失败降级

## 尚未实现

- 窗口、显示器和框选截图
- 屏幕录制权限状态
- 开机启动和系统托盘
- macOS Keychain 与 Linux Secret Service 具体实现

截图方法目前由 `WindowsPlatformIntegration` 返回 `PlatformError::Unsupported`。后续实现必须位于平台 crate，不得把 Windows API 散落到模型、上下文业务层或 Svelte 组件。

## 运行差异

- `Ctrl + Shift + Space` 被占用时会输出警告，但应用继续运行。
- 多显示器布局改变后，保存位置会限制到主显示器可见工作区。
- Windows 缩放切换可能触发多次移动事件，Store 写入已做 220ms 防抖。
- 删除非活动 Profile 时会同步删除其 Credential Manager 条目；内置 Mock 与当前活动 Profile 不能删除。
- UI Automation 能力由目标应用决定；虚拟化编辑器可能只公开可见文字，部分应用不公开选择或文档范围。
- VS Code 未启用其屏幕阅读器优化模式时，编辑器正文和选区可能不通过 UI Automation 暴露；DeskAide 不会自动修改该设置。
- 目标应用在发送前关闭、控件失效或采集超过三秒时，该上下文会标记为不可用或失败，但模型请求继续执行。
