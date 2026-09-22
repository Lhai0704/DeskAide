# Live2D 桌面形象

DeskAide 的 Live2D 是表现层：形象显示 thinking/responding/speaking、注视鼠标、播放模型已有动作和表情。它不读取对话内容，不提供 LLM/MCP 参数控制工具，也不采集屏幕、摄像头或麦克风。

## 本地准备

普通 `npm run build` 不需要 SDK，缺少 runtime 时静态/视频和聊天仍可使用。

1. 阅读并接受 [Core 协议](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html) 和 [Open Software 协议](https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html)，从 [Live2D 官方](https://www.live2d.com/en/sdk/download/web/) 取得 Cubism SDK for Web **5-r.5**。
2. 将 SDK 放入 `.local/live2d/sdk`，确认包含 `Framework/src`、`Framework/Shaders/WebGL` 和 `Core/live2dcubismcore.min.js`。
3. 在仓库根目录运行：

```powershell
npm run prepare:live2d -- .local/live2d/sdk
node scripts/setup-live2d-samples.mjs .local/live2d/sdk
npm run test:live2d
npm run build:debug
```

准备脚本只读取本地 SDK，不下载 SDK/模型。可用第二个参数指定合法取得的 Core 文件。输出 runtime、shaders、来源/哈希和 notices 到 `.local/live2d/runtime`。样例脚本复制官方 Haru/Ren 到本地 packs，不将资源加入 Git。它们是测试形象，不是 DeskAide 自有角色。

Debug 应用读取仓库 `.local/live2d`；非 Debug 应用读取 Tauri app-local-data 下的 `live2d`。设置页显示确切目录并提供“打开本地形象目录”和刷新。将完整资源包放在 `packs/<ASCII目录名>/manifest.json`，然后刷新。资源文件使用普通相对路径，不接受 URL、百分号编码、盘符或路径穿越。

```text
live2d/
  runtime/bridge.js
  runtime/live2dcubismcore.min.js
  runtime/shaders/...
  runtime/*LICENSE.md
  runtime/provenance.json
  packs/<目录名>/manifest.json
  packs/<目录名>/preview.png
  packs/<目录名>/model/角色.model3.json
  packs/<目录名>/model/角色.moc3
  packs/<目录名>/model/贴图、motion3、exp3、physics3、pose3...
```

## 许可边界

DeskAide 自有代码使用 MIT；Core、Framework、shader 和模型分别遵守自己的许可，MIT 不覆盖这些第三方资源。不得从 AIRI 或其他项目复制 Core/模型。本地 runtime 不是任意 JS 插件入口，只安装从官方 SDK 构建的文件。

Core 不是 MIT 软件。保留原始声明、SDK/Core LICENSE、Open/Proprietary 协议及模型条款。个人开发不等于可无条件发布：[SDK Release License](https://www.live2d.com/en/sdk/license/) 将可扩展模型应用单独分类，公开发布前需确认 DeskAide 的适用条款。自行提供 Core 不自动免除发布义务。本次仅本地开发验证，不公开分发含 Live2D 的安装包。

官方样例另受 [Sample Data Terms](https://www.live2d.com/en/learn/sample/model-terms/) 与 Free Material Agreement 约束。其他作者的模型需分别取得使用权；不要将角色资源、runtime 或 SDK 提交到本仓库。

## 交互与设置

- 单击反馈并打开/隐藏 Assistant；移动超过 5 个逻辑像素才开始拖动，拖动不会触发点击。
- Windows `GetCursorPos` 按约 30Hz 采样，只传当前坐标到 avatar WebView，不保存轨迹、不监听点击、不联网。窗口外也能注视。
- gaze 使用 DPI/布局归一化、限幅和时间平滑；静止后只有很小的随机 idle 视线。
- 动作使用模型自带文件；缺少状态 mapping 时回 idle。不会播放模型 motion 所附的声音，声音仅来自 DeskAide TTS。
- 身体动作沿用模型的淡入/淡出设置；语义状态解析到同一动作时不重新启动。短回复完成后允许正在播放的回复动作自然结束（最多 8 秒），然后回 idle；新问题可以打断它。speaking 未单独映射时沿用 responding 动作。嘴型仍随实际声音立即停止，不等待身体动作结束。
- 自动眨眼尊重 motion/expression 眼部参数。关闭开关只关闭额外控制，不能移除原生动作内的眨眼。
- TTS 在输出音量之后提取 RMS，attack/release 平滑后驱动口型。没有 TTS 时不产生程序化口型。停止朗读立即归零；模型原生表情/动作可能有自己的口部姿态。
- 设置按 pack 保存到 Tauri Store，立即生效；缩放不会增大桌面窗口。

## 性能、错误与兼容范围

使用官方 Framework 5-r.5 + Core 的 **WebGL2** 路径，不依赖 Pixi。SDK 支持范围与 DeskAide 实测范围不同：`.model3.json` 不表示所有 Cubism 功能一定兼容。当前开发样例为 Haru（MOC version 1）和 Ren（MOC version 6）；不要由此宣称所有 Cubism 4/5 导出模型都通过验证。

默认目标约 60 FPS。画布按屏幕 DPR 设置，模型先画到宽高各 2 倍的离屏缓冲，再将每组 2×2 像素的预乘 RGBA 一起做面积平均。不能只取最高 alpha 像素的颜色，否则不透明区域的嘴唇、衣服细线仍会随采样位置出现或消失；也不人为放大边缘 alpha。隐藏时暂停；关闭全部动画/追踪后按需重绘。切包销毁模型、纹理、动作、监听和 WebGL context。Core JS 在同一个 WebView 中只加载一次；模型实例及分配在 dispose 时释放。贴图在 `createImageBitmap` 解码时显式预乘 alpha（ImageBitmap 上传忽略 `UNPACK_PREMULTIPLY_ALPHA_WEBGL`），并生成 mipmap。裁剪蒙版保留高精细模式、使用最多 2048 的缓冲，降低每个裁剪部件重复清空/重画 4096² 目标的开销。未写 layout 时，缩放 1 表示模型高度为容纳尺寸的 2 倍，模型中心落在窗口底边，上半身铺满窗口。

模型/贴图失败、Core 缺失、WebGL 故障显示可点击 DA fallback，聊天和 TTS 不受影响。可选动作、表情、physics/pose 加载失败跳过对应项。WebGL context 恢复最多自动尝试一次，可切换形象重新加载。

窗口默认由 manifest 决定，最大 640×900 逻辑像素并限制在工作区。**透明边缘仍占用鼠标命中区域**，本版不承诺逐像素穿透。没有透明像素读取、全局鼠标 hook 或 Win32 子类化。

## 验证

`npm run test:live2d` 使用本机 Edge 的真实 WebGL2、官方 Core/模型，检查透明像素、口型和连续切换释放。缺资源直接失败，不静默跳过。报告在 `.local/live2d/verification`，不入 Git。

`node scripts/test-live2d-quality.mjs` 在真实 WebGL2 上检查四种子像素位置的细线、透明边缘覆盖率和预乘颜色混合，并在 100%/150%/200% DPI 渲染本地 Haru/Hiyori，保存 PNG、GPU 名称及帧间隔到 `verification/quality/after`。需要额外准备合法取得的 Hiyori 包；缺资源直接失败。`--baseline` 将当前结果写到 `quality/before` 并跳过新算法断言，便于在修改前取样。

2026-09-22 本机 Edge/Intel UHD Graphics 测试：560×720 CSS 像素，三个 DPI 的 Haru/Hiyori 新版帧间隔中位数均约 16.7 ms；修改前 Haru 为 22.8–24.6 ms。此数据是短时浏览器渲染测量，不代表所有显卡、桌面合成器或模型的表现。新版细线四个位置均输出约 191 灰度（旧版为 0/255/255/255）；四分之一覆盖的白色边缘输出 alpha 64（旧版 226）。实际桌面运动观感和多显示器效果仍需人工验收。

同日 Debug WebView2 实测 Haru/Hiyori 均正常加载并保存截图：150% DPI，canvas 为 831×1071 物理像素，对应 554×714 CSS 像素，未出现 fallback 或 WebGL 错误。测试后恢复原形象设置并关闭测试进程。截图在 `verification/quality/webview-*.png`。

`scripts/test-live2d-webview.mjs` 可连接为测试单独启动的 Debug WebView2（loopback CDP 9227），验证两个模型、窗口尺寸与系统鼠标适配，并恢复原选择。CDP 仅在手动测试启动时设置，不进入发布配置。浏览器测试不能替代实际桌面交互与多显示器视觉确认。

`scripts/test-live2d-speech.mjs` 使用现有本地 TTS 参考声音测试真实播报、模型口部参数、隐藏面板继续播放和停止归零；不保存语音设置或生成音频。`scripts/test-live2d-performance.mjs` 采样指定 Debug 进程及其子进程，报告在本地 verification 目录；它不是应用遥测。

### 本机验证记录（2026-09-21）

- Framework `5-r.5`，官方配套下载端点提供的 Core `6.0.1`（API version `100663297`）。Core 的版本号与模型导出版本号不是同一概念。文件 SHA-256 保存在本机 `runtime/provenance.json`。
- Edge/WebView2 `153.0.4234.48`，Windows，150% DPI，窗口 240×320 逻辑像素。Haru/Ren 实际透明渲染、窗口外 cursor、点击、context lost/recovery、离线 Mock responding→idle 均通过。
- 真实浏览器连续切换 22 次，检查模型/纹理/RAF 清理、resize、pause、取消与加载失败；实际 Web Audio 排程、音量分析、静音和停止通过。
- 本机 Qwen3-TTS 0.6B 试听通过，隐藏面板保持播放，测得模型口部参数峰值约 0.23，停止后 presentation 归零。
- 32 逻辑处理器机器，CDP 调试连接下各采样约 5 秒：static 占单核约 2.5%，Live2D idle+tracking 约 37.4%（整机约 1.2%），关闭所有动画后约 10.6%。avatar WebView 的任务时间分别约 1.7%、10.3%、1.7%。整个 DeskAide 进程树工作集约 672/754/757 MiB；包含多个 WebView 和调试开销，不是模型独占内存或长期基准。

尚需一次实际鼠标拖动与视觉确认；本机没有完成跨不同 DPI 实体显示器的拖拽验收，相关坐标数学已覆盖 100/125/150/200% 和负坐标单元测试。约 60 FPS 和高精细蒙版不代表所有硬件与所有模型都有相同消耗。
