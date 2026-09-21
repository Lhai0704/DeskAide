use super::*;
#[tauri::command]
pub(super) async fn list_available_windows(
    state: State<'_, AppState>,
) -> Result<Vec<TargetWindow>, String> {
    state.platform.list_windows().await.map_err(display_error)
}

#[tauri::command]
pub(super) async fn collect_window_context(
    state: State<'_, AppState>,
    target: TargetWindow,
) -> Result<TextContextDraft, String> {
    let payload = state
        .active_window_text_provider
        .collect(&ContextRequest {
            target: target.clone(),
            sources: vec![ContextSourceType::ActiveWindowText],
        })
        .await
        .map_err(display_error)?;
    let content = payload
        .main_text
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "该窗口没有公开可读取的文字内容".to_owned())?;
    Ok(TextContextDraft {
        id: Uuid::new_v4().to_string(),
        source: ContextSourceType::ActiveWindowText,
        target: Some(target),
        content,
    })
}

#[tauri::command]
pub(super) async fn preview_selected_text_context(
    state: State<'_, AppState>,
) -> Result<TextContextDraft, String> {
    let target = state
        .context_target
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| "未记录到本次激活前的外部窗口".to_owned())?;
    let payload = state
        .selected_text_provider
        .collect(&ContextRequest {
            target: target.clone(),
            sources: vec![ContextSourceType::SelectedText],
        })
        .await
        .map_err(display_error)?;
    let content = payload
        .selected_text
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "目标控件没有公开选中文字".to_owned())?;
    Ok(TextContextDraft {
        id: Uuid::new_v4().to_string(),
        source: ContextSourceType::SelectedText,
        target: Some(target),
        content,
    })
}

#[tauri::command]
pub(super) async fn preview_clipboard_context(
    state: State<'_, AppState>,
) -> Result<TextContextDraft, String> {
    let content = state
        .platform
        .get_clipboard_text()
        .await
        .map_err(display_error)?
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "剪贴板中没有可添加的文字内容".to_owned())?;
    Ok(TextContextDraft {
        id: Uuid::new_v4().to_string(),
        source: ContextSourceType::Clipboard,
        target: None,
        content,
    })
}

#[tauri::command]
pub(super) fn open_context_editor(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: TextContextDraft,
) -> Result<(), String> {
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(draft.clone());
    let editor = get_window(&app, CONTEXT_EDITOR_LABEL)?;
    editor.show().map_err(display_error)?;
    editor.set_focus().map_err(display_error)?;
    editor
        .emit("context-editor-opened", draft)
        .map_err(display_error)
}

#[tauri::command]
pub(super) fn get_context_editor_draft(state: State<'_, AppState>) -> Option<TextContextDraft> {
    state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[tauri::command]
pub(super) fn save_context_editor_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: TextContextDraft,
) -> Result<(), String> {
    if draft.content.trim().is_empty() {
        return Err("上下文内容不能为空".to_owned());
    }
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(draft.clone());
    app.emit_to(ASSISTANT_LABEL, "context-draft-updated", draft)
        .map_err(display_error)?;
    close_context_editor(app, state)
}

#[tauri::command]
pub(super) fn close_context_editor(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    get_window(&app, CONTEXT_EDITOR_LABEL)?
        .hide()
        .map_err(display_error)?;
    if let Ok(assistant) = get_window(&app, ASSISTANT_LABEL)
        && assistant.is_visible().unwrap_or(false)
    {
        let _ = assistant.set_focus();
    }
    Ok(())
}
