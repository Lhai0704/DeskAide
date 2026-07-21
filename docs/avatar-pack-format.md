# Avatar Pack v1

Avatar Pack 是 DeskAide 的可替换助手形象资源。第一阶段只支持静态 PNG。

## 目录

```text
avatars/<pack-id>/
├─ manifest.json
├─ idle.png
└─ activated.png
```

## Manifest

```json
{
  "schemaVersion": 1,
  "id": "default-assistant",
  "name": "Default Electronic Butler",
  "version": "1.0.0",
  "renderer": "static",
  "defaultWidth": 160,
  "defaultHeight": 160,
  "states": {
    "idle": { "asset": "idle.png", "alt": "助手待机" },
    "activated": { "asset": "activated.png", "alt": "助手已激活" }
  }
}
```

资源路径必须是 Pack 内的相对路径，不允许绝对路径或 `..`。加载器会校验 schema、renderer、尺寸和必需状态；失败时 UI 使用内置文字占位。

未来可以通过新增 renderer 实现动画图片、序列帧、Lottie 或 Live2D，但不得改变 v1 `static` 的含义。

