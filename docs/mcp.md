# 本地 MCP stdio

DeskAide 使用官方 Rust SDK `rmcp 3.4.0` 连接本地 stdio server。这里只提供客户端，不提供 MCP server hosting、HTTP、SSE、WebSocket 或云端服务。

## 配置

1. 自行安装并检查可信的 MCP 程序。DeskAide 不下载或安装包。
2. 设置 → MCP → 添加 server，填写名称、程序和参数。新增配置默认禁用。
3. 参数逐项填写，包含空格的路径直接作为一项，不加 shell 引号。程序不会通过 shell 拼接运行。
4. 可填写存在的绝对工作目录。点击“测试连接”完成初始化和工具发现后，临时进程会被清理。
5. 保存并启用；在模型 Profile 中手动开启“支持工具调用”。只有支持工具的 turn 才按需连接 enabled servers，普通聊天和 Mock 不触发连接。

示例（路径是占位示例，必须替换为实际安装位置）：

| 字段     | Node 程序                          | Python 程序                              |
| -------- | ---------------------------------- | ---------------------------------------- |
| 程序     | `C:\Program Files\nodejs\node.exe` | `D:\McpExample\.venv\Scripts\python.exe` |
| 参数 1   | `D:\McpExample\server.js`          | `D:\McpExample\server.py`                |
| 工作目录 | `D:\McpExample`                    | `D:\McpExample`                          |

不隐式执行 `.cmd` / `.bat`，因此 Windows 的 `npx.cmd` 不能直接使用。建议预先安装程序后使用 `node.exe + 已安装脚本路径`。不提供自定义 env / secret env；不要把密钥放进 command、args、工作目录或普通设置。需要 secret env 的 server 暂不支持。

配置存于本机应用数据目录 `settings.json` 的 `mcpServers`，含稳定 UUID、名称、enabled、command、args、workingDirectory 和自动更新的 revision。编辑、禁用、删除会撤销旧工具、使旧批准失效并停止旧连接。连接状态为未启动、正在连接、已连接、连接失败，显示发现工具数量及不含 server 原始输出的错误说明。

## 运行与批准

工具调用先完整接收模型响应，再校验 JSON 参数与本地 Schema。所有外部 MCP 工具均要求逐次批准，即使 server 自报 readOnly。卡片显示原始工具名称、server 来源、风险类别、参数预览及展开全文；不会渲染服务器提供的 HTML。允许本次不记住永久权限；拒绝会返回结构化错误供模型继续回答。

当本轮包含选中文字、剪贴板或窗口草稿时，**保存参数和结果**是独立的默认关闭选项。允许执行不等于允许保存。未勾选时，模型在本轮仍可使用真实结果；历史只存调用配对、来源、状态和省略标记。勾选仅适用于本次调用。无桌面上下文时，参数和结果正常保存到明文本地历史，并在后续继续该对话时发送给模型服务。模型回答主动引用的内容仍会随回答保存。

多工具按返回顺序串行处理。等待批准期间可以停止生成、发送替代问题或切换对话。取消会结束等待、通知 MCP，必要时关闭该 server 的自有进程；已发生的外部副作用无法回滚，未确认结果明确为 unknown/interrupted，不自动重试。

## 进程生命周期与限制

- 应用启动只加载配置。按需连接；同一 server 并发启动合并，批准和执行期间持有租约。无使用租约且空闲五分钟后停止。
- server 崩溃会撤销工具并显示失败，不无限自动重启。修正程序后在设置中重连。某个 server 失败不阻止普通文本聊天。
- 初始化和工具发现总计 15 秒；工具调用 60 秒；批准等待 5 分钟（超时不允许）；整轮 15 分钟。每轮最多 8 次模型请求、32 次工具调用，每次模型响应最多 16 个工具。
- 最多 16 个配置，每 server 最多 128 个工具、全局 registry 最多 256 个；单个 schema/参数 64 KiB，结果 128 KiB，协议帧 1 MiB。超限返回错误，不截断参数后执行。Schema 外部 `$ref` 和 `$id` 基址不受支持。
- stdout 专用于 MCP 协议。stderr 持续排空但不显示或保存原文，避免 secret 泄漏；排障时请在独立终端自行运行受信任的程序。
- 子进程只继承 PATH、SystemRoot、WINDIR、TEMP、TMP、USERPROFILE、LOCALAPPDATA、APPDATA、PATHEXT 等基础变量；不主动传递模型 API Key。普通配置和路径不能作为凭据存储。
- Windows 使用隐藏控制台、Job Object 和 KillOnDrop 追踪自有进程树。正常退出先关闭 stdio，限时后终止；异常退出由 Job Object 兜底。测试 fixture 覆盖父进程与后代清理。

**进程以当前用户权限运行，逐次工具批准不是系统沙箱。** 初始化程序本身可以读写它有权访问的文件或联网；仅配置你信任的可执行文件。MCP 工具本身是否操作系统、执行命令或访问远端由外部程序决定，DeskAide 本次不提供内建 computer use、Shell Agent 或 remote MCP 客户端。

## 历史与验证

v1 历史迁移至独立 `conversation-history-v2.json`，保留原文件；损坏/未知版本不会被空历史覆盖。工具记录完整配对，重启后中断调用不会自动重执行。Context Registry 不是记忆，未实现 memory 或自动提取记忆。

自动化使用本地 HTTP fixture 和 Rust stdio fixture，不读取真实用户历史，不需要 API Key、第三方 server 或 GPU。覆盖握手、分页发现、多步循环、批准、拒绝、取消、超时、超大帧、进程异常与清理，以及原子历史重载后的模型请求。
