use deskaide_assistant_core::ToolCall;
use deskaide_mcp_client::{McpManager, McpServerConfig};
use deskaide_tool_core::{ToolDiscovery, ToolRegistry};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn config(mode: &str) -> McpServerConfig {
    McpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: "fixture".into(),
        enabled: true,
        command: env!("CARGO_BIN_EXE_mcp-fixture").into(),
        args: vec![mode.into()],
        working_directory: None,
        revision: 1,
    }
}
#[tokio::test]
async fn initialize_discover_call_and_cleanup() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    manager.configure(vec![config("paginate")]).await.unwrap();
    let lease = manager.acquire(CancellationToken::new()).await;
    assert!(lease.warnings.is_empty(), "{:?}", lease.warnings);
    assert_eq!(lease.definitions.len(), 2);
    let d = lease
        .definitions
        .iter()
        .find(|d| d.description == "echo fixture")
        .unwrap();
    let result = registry
        .execute(
            &ToolCall {
                id: "call".into(),
                name: d.name.clone(),
                arguments: "{\"text\":\"hello\"}".into(),
            },
            d,
            CancellationToken::new(),
        )
        .await;
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(
        result.content["structuredContent"]["arguments"]["text"],
        "hello"
    );
    let pid = result.content["structuredContent"]["pid"].as_u64().unwrap() as u32;
    drop(lease);
    manager.shutdown().await;
    assert!(registry.list().is_empty());
    assert!(manager.statuses().iter().all(|s| s.state == "stopped"));
    assert_process_exited(pid);
}
#[cfg(windows)]
fn assert_process_exited(pid: u32) {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            assert_eq!(WaitForSingleObject(handle, 1000), WAIT_OBJECT_0);
            let _ = CloseHandle(handle);
        }
    }
}
#[cfg(not(windows))]
fn assert_process_exited(_pid: u32) {}
#[tokio::test]
async fn process_failure_and_invalid_frame_do_not_leave_tools() {
    for mode in ["crash", "oversize"] {
        let registry = Arc::new(ToolRegistry::default());
        let manager = McpManager::new(registry.clone());
        manager.configure(vec![config(mode)]).await.unwrap();
        let lease = manager.acquire(CancellationToken::new()).await;
        assert!(lease.definitions.is_empty());
        assert!(!lease.warnings.is_empty());
        manager.shutdown().await;
    }
}
#[tokio::test]
async fn cancellation_and_configuration_change_invalidate_old_tools() {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    let mut c = config("call-hang");
    c.args.push(listener.local_addr().unwrap().to_string());
    manager.configure(vec![c]).await.unwrap();
    let lease = manager.acquire(CancellationToken::new()).await;
    let d = lease.definitions[0].clone();
    let token = CancellationToken::new();
    let call = ToolCall {
        id: "c".into(),
        name: d.name.clone(),
        arguments: "{}".into(),
    };
    let work = registry.execute(&call, &d, token.clone());
    tokio::pin!(work);
    tokio::select! {
        result=&mut work=>panic!("unexpected result: {:?}",result.error),
        connection=tokio::time::timeout(Duration::from_secs(3),listener.accept())=>{let (mut stream,_)=connection.unwrap().unwrap();let mut text=String::new();stream.read_to_string(&mut text).await.unwrap();assert_eq!(text,"started");}
    }
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(3), work)
        .await
        .unwrap();
    assert!(result.error.is_some());
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).await.unwrap();
    assert_eq!(text, "cancelled");
    manager.configure(vec![]).await.unwrap();
    assert!(registry.lookup(&d.name).is_none());
    manager.shutdown().await;
}

#[tokio::test]
async fn test_connections_are_temporary_and_malformed_config_is_rejected() {
    assert_eq!(McpManager::test(config("normal")).await.unwrap(), 1);
    let mut bad = config("normal");
    bad.command = "npx.cmd".into();
    assert!(bad.validate().is_err());
    assert!(McpManager::test(config("crash")).await.is_err());
}
#[tokio::test]
async fn initialize_timeout_is_bounded() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry);
    manager.configure(vec![config("hang")]).await.unwrap();
    let lease = tokio::time::timeout(
        Duration::from_secs(18),
        manager.acquire(CancellationToken::new()),
    )
    .await
    .unwrap();
    assert_eq!(lease.warnings[0].code, "mcp_timeout");
    manager.shutdown().await;
}

#[cfg(windows)]
#[tokio::test]
async fn cleanup_terminates_owned_descendants_and_disabled_servers_are_not_discovered() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    let mut c = config("tree");
    c.enabled = false;
    let id = c.id.clone();
    manager.configure(vec![c]).await.unwrap();
    assert!(
        manager
            .acquire(CancellationToken::new())
            .await
            .definitions
            .is_empty()
    );
    manager.reconnect(&id).await.unwrap();
    let definition = registry.list()[0].clone();
    assert!(
        manager
            .acquire(CancellationToken::new())
            .await
            .definitions
            .is_empty()
    );
    let result = registry
        .execute(
            &ToolCall {
                id: "c".into(),
                name: definition.name.clone(),
                arguments: "{}".into(),
            },
            &definition,
            CancellationToken::new(),
        )
        .await;
    let parent = result.content["structuredContent"]["pid"].as_u64().unwrap() as u32;
    let child = result.content["structuredContent"]["childPid"]
        .as_u64()
        .unwrap() as u32;
    manager.shutdown().await;
    assert_process_exited(parent);
    assert_process_exited(child);
}
#[tokio::test]
async fn process_exit_during_call_returns_error_and_releases_tools() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    manager.configure(vec![config("call-crash")]).await.unwrap();
    let lease = manager.acquire(CancellationToken::new()).await;
    let d = &lease.definitions[0];
    let result = registry
        .execute(
            &ToolCall {
                id: "c".into(),
                name: d.name.clone(),
                arguments: "{}".into(),
            },
            d,
            CancellationToken::new(),
        )
        .await;
    assert!(result.error.is_some());
    manager.shutdown().await;
    assert!(registry.list().is_empty());
}

#[tokio::test]
async fn idle_cleanup_waits_for_approval_and_execution_leases() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    manager.configure(vec![config("normal")]).await.unwrap();
    let lease = manager.acquire(CancellationToken::new()).await;
    assert_eq!(lease.definitions.len(), 1);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(301)).await;
    assert_eq!(manager.statuses()[0].state, "ready");
    let mut changes = registry.subscribe_changes();
    drop(lease);
    tokio::time::advance(Duration::from_secs(301)).await;
    changes.changed().await.unwrap();
    assert!(registry.list().is_empty());
    tokio::time::resume();
    manager.shutdown().await;
}

#[tokio::test]
async fn reconnect_invalidates_definition_even_when_configuration_is_unchanged() {
    let registry = Arc::new(ToolRegistry::default());
    let manager = McpManager::new(registry.clone());
    let c = config("normal");
    let id = c.id.clone();
    manager.configure(vec![c]).await.unwrap();
    let lease = manager.acquire(CancellationToken::new()).await;
    let old = lease.definitions[0].clone();
    manager.reconnect(&id).await.unwrap();
    let current = registry.lookup(&old.name).unwrap();
    assert_ne!(current.revision, old.revision);
    let result = registry
        .execute(
            &ToolCall {
                id: "c".into(),
                name: old.name.clone(),
                arguments: "{}".into(),
            },
            &old,
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result.error.unwrap().code, "tool_changed");
    manager.shutdown().await;
}
