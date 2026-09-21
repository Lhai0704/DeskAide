# Avatar Pack v1 / v2 / v3

v1 `static` 和 v2 `video` 保持兼容。共同字段：`id/name/version/defaultWidth/defaultHeight`。尺寸为逻辑像素，最终窗口受工作区和 360×480 上限约束。

## Static / Video

```json
{
  "schemaVersion": 1,
  "renderer": "static",
  "id": "default-assistant",
  "name": "机器人助手",
  "version": "1.0.0",
  "defaultWidth": 160,
  "defaultHeight": 160,
  "states": {
    "idle": { "asset": "idle.png", "alt": "助手" },
    "activated": { "asset": "activated.png", "alt": "助手已激活" }
  }
}
```

视频包改为 `schemaVersion: 2`、`renderer: "video"`，asset 使用本地视频；保持静音循环。static/video 的其他语义状态回退到 idle，不要求补齐图片。

## Live2D

```json
{
  "schemaVersion": 3,
  "renderer": "live2d",
  "id": "my-assistant",
  "name": "我的助手",
  "version": "1.0.0",
  "alt": "桌面助手",
  "preview": "preview.png",
  "defaultWidth": 240,
  "defaultHeight": 320,
  "model": "model/assistant.model3.json",
  "layout": { "scale": 1, "anchor": { "x": 0.5, "y": 0.5 }, "position": { "x": 0.5, "y": 0.5 } },
  "motions": { "idle": { "group": "Idle", "index": 0 }, "activated": { "group": "TapBody", "index": 0 } },
  "expressions": { "neutral": "Neutral", "thinking": "Thinking", "tap": "Smile" },
  "behavior": { "mouseTracking": true, "idleAnimation": true, "motions": true, "blink": "auto" },
  "metadata": { "author": "模型作者", "license": "模型许可说明" }
}
```

除了基础信息、alt、preview、model 和窗口尺寸，其余字段可省略。layout 在 contain 布局上应用，anchor/position 为 0..1，scale 为大于 0 且不超过 3。用户设置的缩放在该基础上叠加。

motions 支持 `idle/activated/thinking/responding/speaking/error`，group/index 引用模型已有动作，index 从 0 起。缺失映射回 idle；没有 idle 动作则继续 gaze/blink。expression 引用模型定义的名字，不由 LLM 生成。恢复 neutral 时释放旧表情。可选 `taps` 按模型 hit-area 名称指定 `{ motion: {group,index}, expression: "名称" }`，只增加视觉反馈。

blink 策略为 auto/model/fallback/off；auto 根据模型眼部参数及动画判断是否添加眨眼，model 尊重已有动画，fallback 为无可靠原生眨眼的模型提供补充，off 不添加自动眨眼。补充控制不覆盖正在控制眼睛的 motion/expression。

路径只能是 pack 内相对路径。manifest 和 model3 的嵌套资源引用都检查：拒绝 `..`、绝对路径、盘符、反斜线、URL、百分号编码和查询参数。Rust 再校验 canonical path，拒绝越界链接。模型不能指定 runtime、脚本或网络地址。必需资源失败显示 fallback，可选动作/表情失败单独降级。

本地目录、SDK 准备、许可、性能与限制见 [Live2D](live2d.md)。
