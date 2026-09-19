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
- 可选的本地 Qwen3-TTS 流式播报、声音选择与即时停止朗读

## 尚未实现

- 窗口、显示器和框选截图
- 屏幕录制权限状态
- 开机启动和系统托盘
- macOS Keychain 与 Linux Secret Service 具体实现
- 语音输入、远程 TTS 服务连接和自动下载语音模型

截图方法目前由 `WindowsPlatformIntegration` 返回 `PlatformError::Unsupported`。后续实现必须位于平台 crate，不得把 Windows API 散落到模型、上下文业务层或 Svelte 组件。

## 运行差异

- `Ctrl + Shift + Space` 被占用时会输出警告，但应用继续运行。
- 多显示器布局改变后，保存位置会限制到主显示器可见工作区。
- Windows 缩放切换可能触发多次移动事件，Store 写入已做 220ms 防抖。
- 删除非活动 Profile 时会同步删除其 Credential Manager 条目；内置 Mock 与当前活动 Profile 不能删除。
- UI Automation 能力由目标应用决定；虚拟化编辑器可能只公开可见文字，部分应用不公开选择或文档范围。
- VS Code 未启用其屏幕阅读器优化模式时，编辑器正文和选区可能不通过 UI Automation 暴露；DeskAide 不会自动修改该设置。
- 目标应用在发送前关闭、控件失效或采集超过三秒时，该上下文会标记为不可用或失败，但模型请求继续执行。
- 语音需要已配置好依赖、模型和参考素材的 fast-qwen3-tts 项目；DeskAide 不负责安装模型或更改系统 Python 环境。
- 首次播报包含模型加载和预热，不能保证固定首音延迟。隐藏面板后继续朗读，直至播完或用户停止。
- 停止播放立即生效；正在执行的 GPU 运算要到加载结束或分块检查点才能取消。服务一次只能执行一个生成/加载任务，网页工作台与 DeskAide 会共享这一限制。
