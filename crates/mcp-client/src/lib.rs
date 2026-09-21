//! Local stdio MCP clients only. Protocol parsing/negotiation is owned by rmcp.
use async_trait::async_trait;
use deskaide_assistant_core::*;
use deskaide_tool_core::{ToolDiscovery, ToolExecutor, ToolLease, ToolRegistry};
use futures_util::StreamExt;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use rmcp::{
    RoleClient, ServiceExt,
    model::{
        CallToolRequest, CallToolRequestParams, ClientRequest, PaginatedRequestParams, ServerResult,
    },
    service::{Peer, PeerRequestOptions},
    transport::async_rw::JsonRpcMessageCodec,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::watch, time::Instant};
use tokio_util::{
    codec::{FramedRead, FramedWrite},
    sync::CancellationToken,
};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const FRAME_LIMIT: usize = 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub command: String,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    #[serde(default)]
    pub revision: u64,
}
impl McpServerConfig {
    pub fn validate(&self) -> Result<(), ToolError> {
        if uuid::Uuid::parse_str(&self.id).is_err()
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.command.is_empty()
            || self.command.contains(['\n', '\r', '\0'])
            || self.args.len() > 64
            || self.args.iter().any(|a| a.len() > 8192 || a.contains('\0'))
        {
            return Err(error("invalid_mcp_config", "MCP 名称、程序或参数无效"));
        }
        if Path::new(&self.command)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        {
            return Err(error(
                "shell_not_supported",
                "请使用原生可执行文件，例如 node.exe 加脚本路径；不支持批处理命令",
            ));
        }
        if let Some(cwd) = &self.working_directory
            && (!Path::new(cwd).is_absolute() || !Path::new(cwd).is_dir())
        {
            return Err(error(
                "invalid_working_directory",
                "工作目录必须是存在的绝对目录",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub id: String,
    pub state: String,
    pub tool_count: usize,
    pub error: Option<String>,
}
struct Connection {
    peer: Peer<RoleClient>,
    stop: CancellationToken,
    alive: AtomicBool,
    leases: AtomicUsize,
    last_used: Mutex<Instant>,
    done: watch::Receiver<bool>,
}
struct Slot {
    config: McpServerConfig,
    connection: tokio::sync::Mutex<Option<Arc<Connection>>>,
    status: Mutex<McpStatus>,
    retired: AtomicBool,
}
pub struct McpManager {
    registry: Arc<ToolRegistry>,
    slots: Mutex<BTreeMap<String, Arc<Slot>>>,
    config_gate: tokio::sync::Mutex<()>,
    next_generation: AtomicU64,
}
fn error(code: &str, message: &str) -> ToolError {
    ToolError::new(code, message)
}
fn alias(server: &str, name: &str) -> String {
    let hash = Sha256::digest(format!("{server}\0{name}").as_bytes());
    format!("mcp_{hash:x}")[..60].into()
}
impl McpManager {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            slots: Mutex::new(BTreeMap::new()),
            config_gate: tokio::sync::Mutex::new(()),
            next_generation: AtomicU64::new(0),
        }
    }
    pub fn configurations(&self) -> Vec<McpServerConfig> {
        self.slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|s| s.config.clone())
            .collect()
    }
    pub fn statuses(&self) -> Vec<McpStatus> {
        self.slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|s| s.status.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .collect()
    }
    pub async fn configure(&self, configs: Vec<McpServerConfig>) -> Result<(), ToolError> {
        if configs.len() > 16 {
            return Err(error("server_limit", "最多配置 16 个 MCP server"));
        }
        let mut ids = HashSet::new();
        for c in &configs {
            c.validate()?;
            if !ids.insert(&c.id) {
                return Err(error("duplicate_server", "MCP ID 重复"));
            }
        }
        let _gate = self.config_gate.lock().await;
        let old: Vec<_> = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        for s in old {
            if !configs.contains(&s.config) {
                s.retired.store(true, Ordering::SeqCst);
                self.registry.unregister_owner(&s.config.id);
                self.stop_slot(&s).await;
                self.slots
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&s.config.id);
            }
        }
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        for config in configs {
            slots.entry(config.id.clone()).or_insert_with(|| {
                Arc::new(Slot {
                    status: Mutex::new(McpStatus {
                        id: config.id.clone(),
                        state: "stopped".into(),
                        tool_count: 0,
                        error: None,
                    }),
                    config,
                    connection: tokio::sync::Mutex::new(None),
                    retired: AtomicBool::new(false),
                })
            });
        }
        Ok(())
    }
    async fn stop_slot(&self, s: &Arc<Slot>) {
        let mut lock = s.connection.lock().await;
        if let Some(c) = lock.take() {
            c.stop.cancel();
            let mut done = c.done.clone();
            let _ = tokio::time::timeout(Duration::from_secs(4), done.wait_for(|v| *v)).await;
        }
    }
    pub async fn shutdown(&self) {
        let slots: Vec<_> = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        for slot in slots {
            slot.retired.store(true, Ordering::SeqCst);
            self.registry.unregister_owner(&slot.config.id);
            self.stop_slot(&slot).await;
        }
    }
    pub async fn reconnect(&self, id: &str) -> Result<(), ToolError> {
        let _gate = self.config_gate.lock().await;
        let slot = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
            .ok_or_else(|| error("unknown_server", "MCP server 不存在"))?;
        self.registry.unregister_owner(id);
        self.stop_slot(&slot).await;
        *slot.status.lock().unwrap_or_else(|e| e.into_inner()) = McpStatus {
            id: id.into(),
            state: "stopped".into(),
            tool_count: 0,
            error: None,
        };
        self.connect(&slot, CancellationToken::new())
            .await
            .map(|_| ())
    }
    pub async fn test(config: McpServerConfig) -> Result<usize, ToolError> {
        config.validate()?;
        let registry = Arc::new(ToolRegistry::default());
        let manager = Self::new(registry);
        let id = config.id.clone();
        manager.configure(vec![config]).await?;
        let result = manager
            .reconnect(&id)
            .await
            .map(|_| manager.registry.list().len());
        manager.shutdown().await;
        result
    }
    async fn connect(
        &self,
        slot: &Arc<Slot>,
        cancel: CancellationToken,
    ) -> Result<Arc<Connection>, ToolError> {
        let mut lock = slot.connection.lock().await;
        if slot.retired.load(Ordering::SeqCst) {
            return Err(error("server_changed", "MCP 配置已更改"));
        }
        if let Some(c) = lock.as_ref() {
            if c.alive.load(Ordering::SeqCst) && !c.stop.is_cancelled() {
                return Ok(Arc::clone(c));
            }
            // Await the old supervisor before registering a new generation for this owner.
            // Otherwise its final unregister/status update could erase the new connection.
            let mut done = c.done.clone();
            tokio::select! {
                _=cancel.cancelled()=>return Err(error("cancelled","连接已取消")),
                result=tokio::time::timeout(Duration::from_secs(4),done.wait_for(|v|*v))=>{
                    if result.is_err(){return Err(error("mcp_cleanup_pending","旧连接仍在清理，请稍后重连"));}
                }
            }
        }
        if slot.status.lock().unwrap_or_else(|e| e.into_inner()).state == "failed" {
            return Err(error(
                "mcp_unavailable",
                "MCP server 不可用，请在设置中重连",
            ));
        }
        slot.status.lock().unwrap_or_else(|e| e.into_inner()).state = "starting".into();
        let result = tokio::select! {
            _=cancel.cancelled()=>Err(error("cancelled","连接已取消")),
            result=tokio::time::timeout(INITIALIZE_TIMEOUT,self.spawn(slot))=>result.unwrap_or_else(|_|Err(error("mcp_timeout","MCP 初始化或工具发现超时"))),
        };
        match result {
            Ok(c) => {
                *lock = Some(Arc::clone(&c));
                Ok(c)
            }
            Err(e) => {
                self.registry.unregister_owner(&slot.config.id);
                let mut status = slot.status.lock().unwrap_or_else(|e| e.into_inner());
                status.state = if e.code == "cancelled" {
                    "stopped"
                } else {
                    "failed"
                }
                .into();
                status.error = Some(e.message.clone());
                status.tool_count = 0;
                Err(e)
            }
        }
    }
    async fn spawn(&self, slot: &Arc<Slot>) -> Result<Arc<Connection>, ToolError> {
        let config = &slot.config;
        let mut command = CommandWrap::with_new(&config.command, |cmd| {
            cmd.args(&config.args)
                .env_clear()
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            for key in [
                "PATH",
                "SystemRoot",
                "WINDIR",
                "TEMP",
                "TMP",
                "USERPROFILE",
                "LOCALAPPDATA",
                "APPDATA",
                "PATHEXT",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    cmd.env(key, value);
                }
            }
            if let Some(cwd) = &config.working_directory {
                cmd.current_dir(cwd);
            }
        });
        command.wrap(KillOnDrop);
        #[cfg(windows)]
        {
            command
                .wrap(process_wrap::tokio::JobObject)
                .wrap(process_wrap::tokio::CreationFlags(
                    windows::Win32::System::Threading::CREATE_NO_WINDOW,
                ));
        }
        let mut child = command
            .spawn()
            .map_err(|_| error("spawn_failed", "无法启动 MCP 程序，请检查可执行文件和参数"))?;
        let stdin = child
            .stdin()
            .take()
            .ok_or_else(|| error("stdio_failed", "无法连接 MCP stdin"))?;
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| error("stdio_failed", "无法连接 MCP stdout"))?;
        let mut stderr = child
            .stderr()
            .take()
            .ok_or_else(|| error("stdio_failed", "无法连接 MCP stderr"))?;
        // Drain without recording arbitrary server output (which can contain credentials).
        let stderr_task = tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0 {
                    break;
                }
            }
        });
        let _stderr_guard = AbortOnDrop(stderr_task.abort_handle());
        let read = FramedRead::new(
            stdout,
            JsonRpcMessageCodec::<rmcp::model::ServerJsonRpcMessage>::new_with_max_length(
                FRAME_LIMIT,
            ),
        );
        let write = FramedWrite::new(
            stdin,
            JsonRpcMessageCodec::<rmcp::model::ClientJsonRpcMessage>::new_with_max_length(
                FRAME_LIMIT,
            ),
        );
        let mut service = ()
            .serve((
                write,
                read.scan((), |_, item| std::future::ready(item.ok())),
            ))
            .await
            .map_err(|_| error("initialize_failed", "MCP 握手失败或协议格式无效"))?;
        let service_stop = service.cancellation_token();
        let mut service_guard = ServiceGuard(Some(service_stop));
        let mut cursor = None;
        let mut cursors = HashSet::new();
        let mut tools = vec![];
        loop {
            let mut params = PaginatedRequestParams::default();
            params.cursor = cursor;
            let page = service
                .peer()
                .list_tools(Some(params))
                .await
                .map_err(|_| error("list_tools_failed", "MCP 工具发现失败"))?;
            tools.extend(page.tools);
            if tools.len() > 128 {
                return Err(error("tool_limit", "单个 MCP server 工具超过 128 个"));
            }
            cursor = page.next_cursor;
            if let Some(c) = &cursor {
                if !cursors.insert(c.clone()) || cursors.len() > 128 {
                    return Err(error("pagination_failed", "MCP 工具分页无效"));
                }
            } else {
                break;
            }
        }
        if slot.retired.load(Ordering::SeqCst) {
            return Err(error("server_changed", "MCP 配置已更改"));
        }
        let (done_tx, done) = watch::channel(false);
        let connection = Arc::new(Connection {
            peer: service.peer().clone(),
            stop: CancellationToken::new(),
            alive: AtomicBool::new(true),
            leases: AtomicUsize::new(0),
            last_used: Mutex::new(Instant::now()),
            done,
        });
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        for tool in &tools {
            let original = tool.name.to_string();
            self.registry
                .register(
                    ToolDefinition {
                        display_name: Some(original.clone()),
                        id: format!("{}:{original}", config.id),
                        name: alias(&config.id, &original),
                        description: tool.description.as_deref().unwrap_or_default().into(),
                        parameters: Value::Object((*tool.input_schema).clone()),
                        source: ToolSource::Mcp {
                            server_id: config.id.clone(),
                            server_name: config.name.clone(),
                        },
                        risk: ToolRisk::Unknown,
                        revision: generation,
                    },
                    Arc::new(McpExecutor {
                        connection: Arc::clone(&connection),
                        name: original,
                    }),
                )
                .map_err(|_| error("invalid_tools", "MCP 提供了重复、过大或不受支持的工具定义"))?;
        }
        *slot.status.lock().unwrap_or_else(|e| e.into_inner()) = McpStatus {
            id: config.id.clone(),
            state: "ready".into(),
            tool_count: tools.len(),
            error: None,
        };
        let c = Arc::clone(&connection);
        let slot = Arc::clone(slot);
        let registry = Arc::clone(&self.registry);
        service_guard.0 = None;
        tokio::spawn(async move {
            let _stderr_guard = _stderr_guard;
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            let failed = loop {
                tokio::select! {
                    _=c.stop.cancelled()=>break false,
                    _=tick.tick()=>{
                        if service.is_closed() || child.try_wait().ok().flatten().is_some() {break true;}
                        let last_used=c.last_used.lock().unwrap_or_else(|e|e.into_inner());
                        if c.leases.load(Ordering::SeqCst)==0 && last_used.elapsed()>=IDLE_TIMEOUT {c.alive.store(false,Ordering::SeqCst);break false;}
                    }
                }
            };
            c.alive.store(false, Ordering::SeqCst);
            registry.unregister_owner(&slot.config.id);
            let _ = service.close_with_timeout(Duration::from_secs(1)).await;
            // EOF is the graceful stdio shutdown. A bounded wait is followed by Job Object kill.
            if tokio::time::timeout(Duration::from_secs(1), child.wait())
                .await
                .is_err()
            {
                let _ = child.start_kill();
                let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
            }
            let mut status = slot.status.lock().unwrap_or_else(|e| e.into_inner());
            status.state = if failed { "failed" } else { "stopped" }.into();
            status.tool_count = 0;
            status.error = failed.then(|| "MCP 进程已退出，请检查程序后重连".into());
            let _ = done_tx.send(true);
        });
        Ok(connection)
    }
}
struct AbortOnDrop(tokio::task::AbortHandle);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct ServiceGuard(Option<rmcp::service::RunningServiceCancellationToken>);
impl Drop for ServiceGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.take() {
            c.cancel();
        }
    }
}
struct Leases(Vec<Arc<Connection>>);
impl Drop for Leases {
    fn drop(&mut self) {
        for c in &self.0 {
            *c.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            c.leases.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
#[async_trait]
impl ToolDiscovery for McpManager {
    async fn acquire(&self, cancel: CancellationToken) -> ToolLease {
        let slots: Vec<_> = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|s| s.config.enabled)
            .cloned()
            .collect();
        let results =
            futures_util::future::join_all(slots.iter().map(|s| self.connect(s, cancel.clone())))
                .await;
        let mut leases = Leases(vec![]);
        let mut warnings = vec![];
        let mut available = HashSet::new();
        for (slot, result) in slots.iter().zip(results) {
            match result {
                Ok(c) => {
                    let mut last_used = c.last_used.lock().unwrap_or_else(|e| e.into_inner());
                    if !c.alive.load(Ordering::SeqCst) || c.stop.is_cancelled() {
                        warnings.push(error(
                            "mcp_unavailable",
                            "MCP 连接已结束，本轮跳过该工具来源",
                        ));
                        continue;
                    }
                    c.leases.fetch_add(1, Ordering::SeqCst);
                    *last_used = Instant::now();
                    drop(last_used);
                    leases.0.push(c);
                    available.insert(slot.config.id.clone());
                }
                Err(e) => warnings.push(e),
            }
        }
        let definitions = self
            .registry
            .list()
            .into_iter()
            .filter(|d| match &d.source {
                ToolSource::Internal => true,
                ToolSource::Mcp { server_id, .. } => available.contains(server_id),
            })
            .collect();
        ToolLease {
            definitions,
            warnings,
            guard: Some(Box::new(leases)),
        }
    }
}
struct McpExecutor {
    connection: Arc<Connection>,
    name: String,
}
#[async_trait]
impl ToolExecutor for McpExecutor {
    async fn execute(&self, args: Value, cancel: CancellationToken) -> ToolResult {
        let connection = Arc::clone(&self.connection);
        let name = self.name.clone();
        // Own cleanup independently of the caller future being dropped by turn cancellation.
        let task = tokio::spawn(async move {
            if !connection.alive.load(Ordering::SeqCst) || connection.stop.is_cancelled() {
                return ToolResult::failure("mcp_unavailable", "MCP 连接已失效");
            }
            let params = CallToolRequestParams::new(name)
                .with_arguments(args.as_object().cloned().unwrap_or_default());
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
            let handle = tokio::select! {
                _=cancel.cancelled()=>return ToolResult::failure("cancelled","工具已取消"),
                r=connection.peer.send_cancellable_request(request,PeerRequestOptions::with_timeout(Duration::from_secs(60)))=>match r {Ok(h)=>h,Err(_)=>return ToolResult::failure("mcp_call_failed","MCP 调用失败")}
            };
            let request_id = handle.id.clone();
            let result = tokio::select! {
                _=cancel.cancelled()=>{
                    let params=serde_json::from_value::<rmcp::model::CancelledNotificationParam>(serde_json::json!({"requestId":request_id,"reason":"DeskAide turn cancelled"}));
                    if let Ok(params)=params {
                    let _=tokio::time::timeout(Duration::from_millis(250),connection.peer.notify_cancelled(params)).await; }
                    connection.stop.cancel();
                    return ToolResult::failure("cancelled_unknown","工具已取消，外部操作结果可能未知");
                },
                r=handle.await_response()=>r,
            };
            match result {
                Ok(ServerResult::CallToolResult(result)) => {
                    let failed = result.is_error.unwrap_or(false);
                    let value = serde_json::to_value(result).unwrap_or(Value::Null);
                    if value.to_string().len() > MAX_TOOL_RESULT_BYTES {
                        return ToolResult::failure("result_too_large", "工具结果过大");
                    }
                    ToolResult {
                        content: value,
                        error: failed.then(|| error("mcp_tool_error", "MCP 工具报告执行失败")),
                    }
                }
                _ => ToolResult::failure("mcp_call_failed", "MCP 调用失败或超时，执行结果可能未知"),
            }
        });
        task.await
            .unwrap_or_else(|_| ToolResult::failure("mcp_task_failed", "MCP 调用任务异常结束"))
    }
}
