use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, State, ipc::Channel};
use tauri_plugin_store::StoreExt;

const BASE: &str = "http://127.0.0.1:7860";
const MAX_LINE: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct StreamDecoder {
    pending: Vec<u8>,
    done: bool,
}
impl StreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, String> {
        self.pending.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(index) = self.pending.iter().position(|b| *b == b'\n') {
            if index > MAX_LINE {
                return Err("TTS 音频片段过大".into());
            }
            let line: Vec<_> = self.pending.drain(..=index).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if self.done {
                return Err("TTS 结束后仍返回数据".into());
            }
            let event: Value = serde_json::from_slice(&line).map_err(|_| "TTS 流格式无效")?;
            match event["type"].as_str() {
                Some("error") => {
                    return Err(event["error"].as_str().unwrap_or("TTS 生成失败").into());
                }
                Some("cancelled" | "done") => {
                    self.done = true;
                    if event["result"]["truncated"] == true {
                        return Err("语音生成达到长度限制，本轮播报已停止".into());
                    }
                }
                Some("audio" | "start") => {}
                _ => return Err("未知 TTS 事件".into()),
            }
            events.push(event);
        }
        if self.pending.len() > MAX_LINE {
            return Err("TTS 流数据过大".into());
        }
        Ok(events)
    }
    fn finish(self) -> Result<(), String> {
        if self.done && self.pending.iter().all(u8::is_ascii_whitespace) {
            Ok(())
        } else {
            Err("TTS 音频连接中断".into())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeechSettings {
    pub enabled: bool,
    pub volume: f32,
    pub reference: String,
    pub model: String,
    pub project_dir: String,
}
impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.8,
            reference: String::new(),
            model: "0.6B".into(),
            project_dir: r"D:\Projects\fast-qwen3-tts".into(),
        }
    }
}
impl SpeechSettings {
    fn validate(&self) -> Result<(), String> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            return Err("音量必须在 0–100% 之间".into());
        }
        if !["0.6B", "1.7B"].contains(&self.model.as_str()) {
            return Err("无效的语音模型".into());
        }
        if !PathBuf::from(&self.project_dir).is_absolute() {
            return Err("项目目录必须是绝对路径".into());
        }
        if self.enabled && self.reference.is_empty() {
            return Err("请先选择参考声音".into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct SpeechState {
    startup: tokio::sync::Mutex<()>,
    child: Mutex<Option<Child>>,
    cancelled: Mutex<VecDeque<String>>,
    active: Mutex<Option<(String, String)>>,
    serial: tokio::sync::Mutex<()>,
}
impl SpeechState {
    fn cancelled(&self, id: &str) -> bool {
        self.cancelled.lock().unwrap().iter().any(|s| s == id)
    }
    pub fn shutdown(&self) {
        // The retained OS child handle is ownership evidence; never kill by port or PID lookup.
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // Windows venv python.exe is a launcher with a runtime child. Terminate
            // that owned tree, not just the launcher, or GPU memory stays allocated.
            #[cfg(windows)]
            if matches!(child.try_wait(), Ok(None)) {
                use std::os::windows::process::CommandExt;
                let _ = Command::new("taskkill.exe")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .creation_flags(0x08000000)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for SpeechState {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())
}
fn compatible(status: &Value) -> bool {
    status["service"] == "fast-qwen3-tts"
        && status["api_version"] == 1
        && ["temporary_stream", "cancel_task"].iter().all(|c| {
            status["capabilities"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(c)))
        })
}
async fn status(client: &reqwest::Client) -> Result<Value, String> {
    let response = client
        .get(format!("{BASE}/status"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: Value = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !compatible(&value) {
        return Err("7860 端口服务不兼容，请更新并手动重启 fast-qwen3-tts".into());
    }
    Ok(value)
}

async fn ensure_service(state: &SpeechState, settings: &SpeechSettings) -> Result<Value, String> {
    let _startup = state.startup.lock().await;
    let http = client()?;
    if let Ok(value) = status(&http).await {
        return Ok(value);
    }
    // A reachable but unrecognised service must never be replaced.
    if std::net::TcpStream::connect_timeout(
        &"127.0.0.1:7860".parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
    {
        return Err("7860 端口已占用或服务版本不兼容，请检查并手动重启 TTS".into());
    }
    let root = PathBuf::from(&settings.project_dir);
    let python = root.join(".venv/Scripts/python.exe");
    if !python.is_file() || !root.join("local_app.py").is_file() {
        return Err("项目目录缺少 .venv/Scripts/python.exe 或 local_app.py".into());
    }
    state.shutdown();
    let mut command = Command::new(python);
    command
        .arg(root.join("local_app.py"))
        .current_dir(&root)
        .env("DESKAIDE_PARENT_PID", std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, path) in [
        ("HF_HOME", "models/huggingface"),
        ("UV_CACHE_DIR", "cache/uv"),
        ("UV_PYTHON_INSTALL_DIR", "runtime"),
        ("TORCH_HOME", "cache/torch"),
        ("XDG_CACHE_HOME", "cache"),
        ("NUMBA_CACHE_DIR", "cache/numba"),
        ("CUDA_CACHE_PATH", "cache/cuda"),
        ("TEMP", "tmp"),
        ("TMP", "tmp"),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        command.env(key, path);
    }
    for key in [
        "HF_HUB_OFFLINE",
        "TRANSFORMERS_OFFLINE",
        "HF_HUB_DISABLE_SYMLINKS_WARNING",
        "HF_HUB_DISABLE_TELEMETRY",
        "HF_HUB_DISABLE_XET",
        "PYTHONUTF8",
        "PYTHONNOUSERSITE",
        "PYTHONUNBUFFERED",
    ] {
        command.env(key, "1");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    *state.child.lock().unwrap() =
        Some(command.spawn().map_err(|e| format!("启动 TTS 失败：{e}"))?);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        if let Ok(value) = status(&http).await {
            return Ok(value);
        }
        if state
            .child
            .lock()
            .unwrap()
            .as_mut()
            .is_none_or(|child| child.try_wait().ok().flatten().is_some())
        {
            return Err("TTS 启动失败，请使用项目 start.cmd 查看依赖错误".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    state.shutdown();
    Err("TTS 启动超时，请检查项目环境".into())
}

#[tauri::command]
pub fn get_speech_settings(app: AppHandle) -> Result<SpeechSettings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store
        .get("speech")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}
#[tauri::command]
pub fn save_speech_settings(app: AppHandle, settings: SpeechSettings) -> Result<(), String> {
    settings.validate()?;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("speech", json!(settings));
    store.save().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn check_speech_service(
    state: State<'_, SpeechState>,
    settings: SpeechSettings,
) -> Result<Value, String> {
    settings.validate()?;
    ensure_service(&state, &settings).await
}
#[tauri::command]
pub async fn speech_references(
    state: State<'_, SpeechState>,
    settings: SpeechSettings,
) -> Result<Value, String> {
    settings.validate()?;
    ensure_service(&state, &settings).await?;
    client()?
        .get(format!("{BASE}/references"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEvent {
    session_id: String,
    request_id: String,
    segment_id: String,
    event: Value,
}

struct ActiveGuard<'a>(&'a SpeechState);
impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        *self.0.active.lock().unwrap() = None;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSegment {
    session_id: String,
    request_id: String,
    segment_id: String,
    text: String,
    settings: SpeechSettings,
}

#[tauri::command]
pub async fn speak_segment(
    state: State<'_, SpeechState>,
    input: SpeechSegment,
    on_event: Channel<SpeechEvent>,
) -> Result<(), String> {
    input.settings.validate()?;
    if input.text.trim().is_empty() || input.text.chars().count() > 500 {
        return Err("播报片段必须为 1–500 字".into());
    }
    for id in [&input.session_id, &input.request_id, &input.segment_id] {
        if id.is_empty() || id.len() > 128 {
            return Err("无效的播报 ID".into());
        }
    }
    let _serial = state.serial.lock().await;
    if state.cancelled(&input.session_id) {
        return Ok(());
    }
    let send = |event: Value| {
        on_event
            .send(SpeechEvent {
                session_id: input.session_id.clone(),
                request_id: input.request_id.clone(),
                segment_id: input.segment_id.clone(),
                event,
            })
            .map_err(|e| e.to_string())
    };
    send(json!({"type":"status", "message":"启动服务 / 检查连接"}))?;
    ensure_service(&state, &input.settings).await?;
    if state.cancelled(&input.session_id) {
        return Ok(());
    }
    let http = client()?;
    let library: Value = http
        .get(format!("{BASE}/references"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let reference = library["references"]
        .as_array()
        .and_then(|a| a.iter().find(|r| r["file"] == input.settings.reference))
        .ok_or("参考声音已不存在，请重新选择")?;
    if state.cancelled(&input.session_id) {
        return Ok(());
    }
    let task_id = uuid::Uuid::new_v4().to_string();
    *state.active.lock().unwrap() = Some((input.session_id.clone(), task_id.clone()));
    let _active = ActiveGuard(&state);
    send(json!({"type":"status", "message":"等待音频"}))?;
    let work = async {
        let response = http.post(format!("{BASE}/generate/stream")).timeout(Duration::from_secs(180)).json(&json!({"text":input.text, "reference":input.settings.reference, "ref_text":reference["text"].as_str().unwrap_or(""), "model":input.settings.model, "persist":false, "task_id":task_id})).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();
        let mut decoder = StreamDecoder::default();
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(180), stream.next())
            .await
            .map_err(|_| "等待 TTS 音频超时")?
        {
            for event in decoder.push(&chunk.map_err(|e| e.to_string())?)? {
                if !state.cancelled(&input.session_id) {
                    send(event)?;
                }
            }
        }
        decoder.finish()
    };
    let cancellation = async {
        loop {
            if state.cancelled(&input.session_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    };
    let result = tokio::select! { result = work => result, () = cancellation => Ok(()) };
    if state.cancelled(&input.session_id) || result.is_err() {
        let _ = http
            .post(format!("{BASE}/cancel"))
            .timeout(Duration::from_secs(3))
            .json(&json!({"task_id":task_id}))
            .send()
            .await;
    }
    if state.cancelled(&input.session_id) {
        // Keep our queue serialized until the cancelled GPU work reaches a checkpoint.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        while tokio::time::Instant::now() < deadline {
            match status(&http).await {
                Ok(value) if value["task"]["busy"] == true => {
                    tokio::time::sleep(Duration::from_millis(100)).await
                }
                _ => break,
            }
        }
    }
    result
}

#[tauri::command]
pub async fn cancel_speech(
    state: State<'_, SpeechState>,
    session_id: String,
) -> Result<(), String> {
    {
        let mut cancelled = state.cancelled.lock().unwrap();
        if cancelled.len() >= 1024 {
            cancelled.pop_front();
        }
        cancelled.push_back(session_id.clone());
    }
    let active = state.active.lock().unwrap().clone();
    if let Some((_, task)) = active.filter(|(session, _)| session == &session_id) {
        client()?
            .post(format!("{BASE}/cancel"))
            .timeout(Duration::from_secs(3))
            .json(&json!({"task_id":task}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_split_utf8_and_rejects_incomplete_streams() {
        let wire =
            "{\"type\":\"start\"}\n{\"type\":\"audio\",\"label\":\"中文\"}\n{\"type\":\"done\"}\n";
        let mut decoder = StreamDecoder::default();
        let mut events = Vec::new();
        for byte in wire.as_bytes() {
            events.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(events.len(), 3);
        assert!(decoder.finish().is_ok());
        assert!(StreamDecoder::default().finish().is_err());
    }
    #[test]
    fn surfaces_busy_errors_and_bounds_stream_frames() {
        assert_eq!(
            StreamDecoder::default()
                .push(b"{\"type\":\"error\",\"error\":\"busy\"}\n")
                .unwrap_err(),
            "busy"
        );
        assert!(
            StreamDecoder::default()
                .push(&vec![b'x'; MAX_LINE + 1])
                .is_err()
        );
        assert!(StreamDecoder::default().push(b"not json\n").is_err());
    }
    #[test]
    fn requires_service_identity_and_capabilities() {
        assert!(!compatible(&json!({"ready":true})));
        assert!(compatible(
            &json!({"service":"fast-qwen3-tts", "api_version":1, "capabilities":["temporary_stream","cancel_task"]})
        ));
    }
    #[test]
    fn rejects_invalid_settings() {
        let mut s = SpeechSettings::default();
        assert!(s.validate().is_ok());
        s.enabled = true;
        assert!(s.validate().is_err());
        s.reference = "voice.wav".into();
        s.volume = f32::NAN;
        assert!(s.validate().is_err());
    }
}
