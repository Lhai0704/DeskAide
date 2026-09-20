# 语音播报验证记录

2026-09-19，Windows 实机，独立 Debug EXE + WebView2；NVIDIA RTX 4060 Laptop GPU 8 GB。界面通过 agent-browser 的 CDP 连接检查，使用本地测试参考素材。此记录不包含参考素材名称、录音、聊天正文或用户配置。

## 自动检查

- 前端：`npm run lint`、`npm run check`、`npm test` 通过，45 项测试；新增分句、Markdown 增量过滤、完成事件去重、播放时序、缓冲控制及取消后迟到音频检查。
- Rust：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 通过，55 项测试。
- TTS：`python -m unittest test_studio test_stream test_temporary -v` 通过，19 项测试；`node --test test_stream_player.js` 通过，4 项；Python 编译检查通过。
- `npm run build:debug` 产出 `target/debug/deskaide.exe`；两个仓库 `git diff --check` 通过。
- 发布前统一了工作区中 7 个文件的换行符，并通过 `.gitattributes` 固定文本检出为 LF；这些文件没有代码内容变化。全仓 `npm run format:check` 通过，Windows 检出与格式检查使用相同换行约定。

## 实机播报

首音指 Web Audio 开始排入第一块音频的时间，另有约 150 ms 初始播放缓冲。以下为单次观测，不是性能保证。

| 场景 | 首块音频 | 接收音频 | 结果 |
| --- | ---: | ---: | --- |
| 0.6B 首次加载模型 | 10.90 s | 5.60 s / 7 块 | 完整播放 |
| 0.6B 模型已加载 | 0.83 s | 5.20 s / 7 块 | 完整播放 |
| 切换并加载 1.7B | 6.92 s | 4.88 s / 6 块 | 完整播放 |
| 1.7B 模型已加载，隐藏面板 | 1.07 s | 4.96 s / 6 块 | 隐藏时音频时钟继续推进，结束后释放播放器 |
| Mock 长回复 + 1.7B | 1.32 s | 分块继续播放 | 全文在 4.96 s 完成，首块早于全文约 3.64 s |

- 模型回复结束后仍显示“停止朗读”；点击后 AudioContext 立即进入 `closed`。
- 最终版本测试：试听开始后 300 ms 停止，400 ms 再次试听；旧会话没有播放音频，新会话完整播放，首块约 1.18 s。
- 三个并发连接检查返回同一个服务 instance ID，只启动一组 Python 启动器/运行进程。
- 最终版本自启服务加载 0.6B 后退出 DeskAide，Python 启动器与运行进程均退出，7860 端口关闭，GPU 显存读数回到 0 MiB；父进程退出检测覆盖绕过 Tauri 退出回调的关闭路径。
- 另行手动启动服务后再启动 DeskAide，连接返回同一 instance ID；退出 DeskAide 后服务仍可访问。最后仅清理了本次测试启动的手动服务。
- 实机播报前后 TTS `generated` 文件数量保持不变；自动测试另验证不生成 WAV/JSON、不打印正文、不累积完整波形。
- 测试使用本地 Mock 文字模型，没有发送请求到付费或远程文字模型；测试对话已删除，语音设置恢复为测试前默认关闭状态。

## 验收边界

以上验证了真实模型合成、WebView 音频排队与播放时钟、界面操作及请求隔离。音色是否满意、实际扬声器听感与是否存在可感知接缝，仍需用户人工试听确认。
