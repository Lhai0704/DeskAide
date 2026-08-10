<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo, listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    AVATAR_PACK_CHANGED_EVENT,
    loadAvatarPackId,
    saveAvatarPackId,
    type AvatarPackId,
  } from '../avatar/catalog';
  import ModelSettings from '../settings/ModelSettings.svelte';
  import { loadTheme, saveTheme, type Theme } from '../settings/theme';
  import {
    buildModelMessages,
    hasSavableConversation,
    responseToConversationMessage,
    type ConversationMessage,
    type ConversationRecord,
    type SaveConversationInput,
  } from './conversation';
  import {
    initialResponseState,
    reduceResponseEvent,
    type ResponseEvent,
    type ResponseState,
  } from './events';
  import {
    CONTEXT_OPTIONS,
    contextResultNote,
    contextSourceLabel,
    contextUnavailableReason,
    contextExcerpt,
    windowLabel,
    type AssistantBootstrap,
    type AssistantShownPayload,
    type ContextCollectionResult,
    type ContextSourceId,
    type SubmitModelRequestResult,
    type TargetWindow,
    type WindowContextDraft,
  } from './model';
  import HistoryDrawer from './HistoryDrawer.svelte';

  let prompt = '';
  let responseState: ResponseState = initialResponseState();
  let messages: ConversationMessage[] = [];
  let pending = false;
  let stopping = false;
  let expanded = true;
  let pinned = false;
  let contextOpen = false;
  let settingsOpen = false;
  let historyOpen = false;
  let theme: Theme = loadTheme();
  let avatarPackId: AvatarPackId = loadAvatarPackId();
  let bootstrap: AssistantBootstrap | null = null;
  let activeModelProfileId = '';
  let conversationId: string = createId();
  let historySaveError = '';
  let modelRestoreWarning = '';
  let bootstrapError = '';
  let contextWarning = '';
  let activeTarget: TargetWindow | null = null;
  let contextResults: ContextCollectionResult[] = [];
  let availableWindows: TargetWindow[] = [];
  let windowContexts: WindowContextDraft[] = [];
  let loadingWindows = false;
  let collectingWindows = false;
  let contextActionError = '';
  let textarea: HTMLTextAreaElement;
  let conversationElement: HTMLElement;
  let historySaveQueue: Promise<void> = Promise.resolve();
  const ignoredRequestIds = new SvelteSet<string>();
  const selectedContextSources = new SvelteSet<ContextSourceId>();
  const selectedWindowIds = new SvelteSet<string>();
  const selectedTextOption = CONTEXT_OPTIONS.find((option) => option.id === 'selectedText')!;

  onMount(() => {
    const unlistenResponse = listen<ResponseEvent>('model-response', ({ payload }) => {
      if (ignoredRequestIds.has(payload.requestId)) {
        if (
          payload.type === 'completed' ||
          payload.type === 'failed' ||
          payload.type === 'cancelled'
        ) {
          ignoredRequestIds.delete(payload.requestId);
        }
        return;
      }
      const nextResponseState = reduceResponseEvent(responseState, payload);
      responseState = nextResponseState;
      if (
        payload.type === 'completed' ||
        payload.type === 'failed' ||
        payload.type === 'cancelled'
      ) {
        pending = false;
        stopping = false;
        void finalizeResponse(nextResponseState);
      } else {
        void scrollConversationToBottom();
      }
    });
    const unlistenShown = listen<AssistantShownPayload>('assistant-shown', ({ payload }) => {
      activeTarget = payload.target;
      contextWarning = payload.warning ?? '';
      pinned = payload.pinned;
      contextResults = [];
      selectedContextSources.clear();
      window.setTimeout(() => textarea?.focus(), 0);
    });
    const unlistenContextUpdate = listen<WindowContextDraft>(
      'context-draft-updated',
      ({ payload }) => {
        windowContexts = windowContexts.map((draft) => (draft.id === payload.id ? payload : draft));
      },
    );

    void loadBootstrap();
    textarea?.focus();

    return () => {
      void unlistenResponse.then((unlisten) => unlisten());
      void unlistenShown.then((unlisten) => unlisten());
      void unlistenContextUpdate.then((unlisten) => unlisten());
    };
  });

  async function loadBootstrap() {
    try {
      bootstrapError = '';
      bootstrap = await invoke<AssistantBootstrap>('get_assistant_bootstrap');
      activeModelProfileId = bootstrap.activeModelProfileId;
    } catch (cause) {
      bootstrapError = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function activeProfile() {
    return bootstrap?.modelProfiles.find((profile) => profile.id === activeModelProfileId) ?? null;
  }

  async function persistConversation() {
    if (!hasSavableConversation(messages) || !activeModelProfileId) return;
    const savedConversationId = conversationId;
    const conversation: SaveConversationInput = {
      id: savedConversationId,
      title: null,
      modelProfileId: activeModelProfileId,
      messages,
    };
    const saveTask = historySaveQueue.then(async () => {
      try {
        await invoke<ConversationRecord>('save_conversation', { conversation });
        if (conversationId === savedConversationId) historySaveError = '';
      } catch (cause) {
        if (conversationId === savedConversationId) {
          historySaveError = cause instanceof Error ? cause.message : String(cause);
        }
      }
    });
    historySaveQueue = saveTask;
    await saveTask;
  }

  async function finalizeResponse(response: ResponseState) {
    const message = responseToConversationMessage(response, createId());
    if (message) messages = [...messages, message];
    responseState = initialResponseState();
    await persistConversation();
    await scrollConversationToBottom();
  }

  async function submit() {
    const value = prompt.trim();
    if (!value || pending || !activeModelProfileId) return;

    const userMessageId = createId();
    messages = [...messages, { id: userMessageId, role: 'user', content: value }];
    prompt = '';
    pending = true;
    contextResults = [];
    responseState = initialResponseState();
    await persistConversation();
    try {
      const result = await invoke<SubmitModelRequestResult>('submit_model_request', {
        conversationId,
        messages: buildModelMessages(messages),
        contextSources: [...selectedContextSources],
        windowContexts,
      });
      contextResults = result.contextResults;
      selectedContextSources.clear();
      windowContexts = [];
      const note = contextResultNote(result.contextResults);
      if (note) {
        messages = messages.map((message) =>
          message.id === userMessageId ? { ...message, note } : message,
        );
        await persistConversation();
      }
      if (!responseState.requestId)
        responseState = { ...responseState, requestId: result.requestId };
    } catch (cause) {
      pending = false;
      const failedResponse: ResponseState = {
        requestId: null,
        content: '',
        status: 'failed',
        error: cause instanceof Error ? cause.message : String(cause),
      };
      responseState = failedResponse;
      await finalizeResponse(failedResponse);
    }
  }

  async function stop() {
    if (!responseState.requestId || !pending || stopping) return;
    stopping = true;
    try {
      const stopped = await invoke<boolean>('stop_generation', {
        requestId: responseState.requestId,
      });
      if (!stopped) stopping = false;
    } catch (cause) {
      stopping = false;
      responseState = {
        ...responseState,
        status: 'failed',
        error: cause instanceof Error ? cause.message : String(cause),
      };
    }
  }

  async function newConversation() {
    if (pending && !responseState.requestId) return;
    await interruptActiveGeneration();
    resetToBlankConversation();
  }

  function resetToBlankConversation() {
    messages = [];
    responseState = initialResponseState();
    contextResults = [];
    selectedContextSources.clear();
    selectedWindowIds.clear();
    windowContexts = [];
    prompt = '';
    conversationId = createId();
    historySaveError = '';
    modelRestoreWarning = '';
    window.setTimeout(() => textarea?.focus(), 0);
  }

  async function interruptActiveGeneration() {
    if (!pending || !responseState.requestId) return;
    const requestId = responseState.requestId;
    ignoredRequestIds.add(requestId);
    const cancelledResponse: ResponseState = { ...responseState, status: 'cancelled', error: '' };
    pending = false;
    stopping = false;
    const message = responseToConversationMessage(cancelledResponse, createId());
    if (message) messages = [...messages, message];
    responseState = initialResponseState();
    await persistConversation();
    try {
      await invoke<boolean>('stop_generation', { requestId });
    } catch (cause) {
      modelRestoreWarning = `停止生成失败：${cause instanceof Error ? cause.message : String(cause)}`;
    }
  }

  async function openHistory() {
    if (!expanded) {
      await invoke('set_assistant_expanded', { expanded: true });
      expanded = true;
    }
    contextOpen = false;
    settingsOpen = false;
    historyOpen = true;
  }

  async function selectHistoryConversation(selectedConversationId: string) {
    if (selectedConversationId === conversationId) {
      historyOpen = false;
      await scrollConversationToBottom();
      window.setTimeout(() => textarea?.focus(), 0);
      return;
    }
    if (pending && !responseState.requestId) {
      throw new Error('模型请求正在准备，请稍后再切换对话');
    }
    const record = await invoke<ConversationRecord | null>('load_conversation', {
      conversationId: selectedConversationId,
    });
    if (!record) throw new Error('这条历史对话已不存在');

    await interruptActiveGeneration();
    messages = record.messages;
    conversationId = record.id;
    resetTransientConversationState();
    historySaveError = '';
    modelRestoreWarning = '';
    await restoreConversationModel(record.modelProfileId);
    historyOpen = false;
    await scrollConversationToBottom();
    window.setTimeout(() => textarea?.focus(), 0);
  }

  async function restoreConversationModel(modelProfileId: string) {
    if (modelProfileId === activeModelProfileId) return;
    const profileExists = bootstrap?.modelProfiles.some((profile) => profile.id === modelProfileId);
    if (!profileExists) {
      modelRestoreWarning = '原对话使用的模型已不存在，已保留当前模型。';
      return;
    }
    try {
      await invoke('set_active_model_profile', { profileId: modelProfileId });
      activeModelProfileId = modelProfileId;
      if (bootstrap) bootstrap = { ...bootstrap, activeModelProfileId: modelProfileId };
    } catch (cause) {
      modelRestoreWarning = `无法恢复原模型，已保留当前模型：${
        cause instanceof Error ? cause.message : String(cause)
      }`;
    }
  }

  async function deleteHistoryConversation(deletedConversationId: string) {
    if (deletedConversationId === conversationId) {
      if (pending && !responseState.requestId) {
        throw new Error('模型请求正在准备，请稍后再删除当前对话');
      }
      await interruptActiveGeneration();
    }
    await historySaveQueue;
    await invoke<boolean>('delete_conversation', { conversationId: deletedConversationId });
    if (deletedConversationId === conversationId) resetToBlankConversation();
  }

  function resetTransientConversationState() {
    responseState = initialResponseState();
    pending = false;
    stopping = false;
    contextResults = [];
    selectedContextSources.clear();
    selectedWindowIds.clear();
    windowContexts = [];
    availableWindows = [];
    contextActionError = '';
    prompt = '';
  }

  async function scrollConversationToBottom() {
    await tick();
    if (conversationElement) conversationElement.scrollTop = conversationElement.scrollHeight;
  }

  async function toggleExpanded() {
    const next = !expanded;
    await invoke('set_assistant_expanded', { expanded: next });
    expanded = next;
  }

  async function togglePinned() {
    const next = !pinned;
    await invoke('set_assistant_pinned', { pinned: next });
    pinned = next;
  }

  async function openSettings() {
    if (!expanded) {
      await invoke('set_assistant_expanded', { expanded: true });
      expanded = true;
    }
    contextOpen = false;
    historyOpen = false;
    settingsOpen = true;
  }

  async function changeModel(event: Event) {
    const next = (event.currentTarget as HTMLSelectElement).value;
    if (!next || next === activeModelProfileId) return;
    const previous = activeModelProfileId;
    activeModelProfileId = next;
    bootstrapError = '';
    try {
      await invoke('set_active_model_profile', { profileId: next });
      if (bootstrap) bootstrap = { ...bootstrap, activeModelProfileId: next };
    } catch (cause) {
      activeModelProfileId = previous;
      bootstrapError = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function changeTheme(next: Theme) {
    theme = next;
    saveTheme(next);
  }

  function changeAvatar(next: AvatarPackId) {
    avatarPackId = next;
    saveAvatarPackId(next);
    void emitTo('avatar', AVATAR_PACK_CHANGED_EVENT, { packId: next });
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (historyOpen) {
        historyOpen = false;
        return;
      }
      if (settingsOpen) {
        settingsOpen = false;
        return;
      }
      contextOpen = false;
      void invoke('hide_assistant');
    } else if (
      !historyOpen &&
      !settingsOpen &&
      event.key === 'Enter' &&
      (event.ctrlKey || event.metaKey)
    ) {
      event.preventDefault();
      void submit();
    }
  }

  function setContextSelected(source: ContextSourceId, selected: boolean) {
    if (selected) selectedContextSources.add(source);
    else selectedContextSources.delete(source);
  }

  async function toggleContextPicker() {
    contextOpen = !contextOpen;
    contextActionError = '';
    if (contextOpen) await loadAvailableWindows();
  }

  async function loadAvailableWindows() {
    loadingWindows = true;
    contextActionError = '';
    try {
      availableWindows = await invoke<TargetWindow[]>('list_available_windows');
    } catch (cause) {
      contextActionError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loadingWindows = false;
    }
  }

  function setWindowSelected(id: string, selected: boolean) {
    if (selected) selectedWindowIds.add(id);
    else selectedWindowIds.delete(id);
  }

  function hasWindowContext(targetId: string) {
    return windowContexts.some((draft) => draft.target.id === targetId);
  }

  async function addSelectedWindows() {
    const targets = availableWindows.filter(
      (target) => selectedWindowIds.has(target.id) && !hasWindowContext(target.id),
    );
    if (targets.length === 0 || collectingWindows) return;
    collectingWindows = true;
    contextActionError = '';
    const added: WindowContextDraft[] = [];
    let failureCount = 0;
    for (const target of targets) {
      try {
        added.push(await invoke<WindowContextDraft>('collect_window_context', { target }));
      } catch {
        failureCount += 1;
      }
    }
    windowContexts = [...windowContexts, ...added];
    if (failureCount > 0) {
      contextActionError = `${failureCount} 个窗口未能读取；可能是窗口已关闭或未公开可访问文字。`;
    }
    selectedWindowIds.clear();
    collectingWindows = false;
    if (failureCount === 0) contextOpen = false;
  }

  async function editWindowContext(draft: WindowContextDraft) {
    contextOpen = false;
    await invoke('open_context_editor', { draft });
  }

  function removeWindowContext(id: string) {
    windowContexts = windowContexts.filter((draft) => draft.id !== id);
  }

  function targetLabel() {
    if (!activeTarget) return '未记录到外部窗口';
    return (
      activeTarget.title || activeTarget.applicationName || activeTarget.processName || '外部窗口'
    );
  }

  function selectedTextUnavailable() {
    const profile = activeProfile();
    return profile
      ? contextUnavailableReason(selectedTextOption, profile.capabilities, activeTarget)
      : '';
  }

  function createId() {
    return globalThis.crypto?.randomUUID?.() ?? `conversation-${Date.now()}`;
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<main class:expanded class="panel">
  <header>
    <div>
      <p class="eyebrow">DESKTOP ASSISTANT</p>
      <h1>DeskAide</h1>
    </div>
    <div class="header-actions">
      <button class="icon-button" type="button" title="历史对话" onclick={openHistory}>◷</button>
      <button
        class="icon-button"
        type="button"
        title="新建会话"
        disabled={pending && !responseState.requestId}
        onclick={newConversation}>＋</button
      >
      <button class="icon-button" type="button" title="设置" onclick={openSettings}>⚙</button>
      <button
        class="icon-button"
        class:pinned
        type="button"
        title={pinned ? '取消置顶' : '置顶窗口'}
        aria-pressed={pinned}
        onclick={togglePinned}>{pinned ? '📌' : '📍'}</button
      >
      <button
        class="icon-button"
        type="button"
        title={expanded ? '收起为小窗口' : '展开为大窗口'}
        onclick={toggleExpanded}>{expanded ? '↙' : '↗'}</button
      >
      <button
        class="icon-button close"
        type="button"
        aria-label="隐藏助手"
        onclick={() => invoke('hide_assistant')}>×</button
      >
    </div>
  </header>

  <section class="toolbar" aria-label="模型与上下文">
    <label class="model-picker">
      <span>模型</span>
      <select value={activeModelProfileId} onchange={changeModel} disabled={!bootstrap || pending}>
        {#each bootstrap?.modelProfiles ?? [] as profile (profile.id)}
          <option value={profile.id}>{profile.name}</option>
        {/each}
      </select>
    </label>
    <div class="context-picker">
      <button
        class="context-trigger"
        type="button"
        aria-expanded={contextOpen}
        onclick={toggleContextPicker}
      >
        添加上下文 <span>{selectedContextSources.size + windowContexts.length}</span>
      </button>
      {#if contextOpen && activeProfile()}
        <div class="context-menu">
          <div class="context-heading">
            <strong>添加本次上下文</strong>
            <small>可同时选择多个窗口，内容会先采集为可编辑草稿</small>
          </div>
          <label class="context-option available" title={selectedTextUnavailable() || '发送时读取'}>
            <input
              type="checkbox"
              checked={selectedContextSources.has('selectedText')}
              disabled={Boolean(selectedTextUnavailable()) || pending}
              onchange={(event) =>
                setContextSelected(
                  'selectedText',
                  (event.currentTarget as HTMLInputElement).checked,
                )}
            />
            <span>助手激活前的选中文字</span>
            <small>{selectedTextUnavailable() || `来自：${targetLabel()}`}</small>
          </label>

          <div class="window-list-heading">
            <span>选择窗口内容</span>
            <button type="button" onclick={loadAvailableWindows} disabled={loadingWindows}
              >刷新</button
            >
          </div>
          <div class="window-list">
            {#if loadingWindows}
              <small class="window-placeholder">正在查找窗口…</small>
            {:else if availableWindows.length === 0}
              <small class="window-placeholder">没有找到可添加的外部窗口</small>
            {:else}
              {#each availableWindows as target (target.id)}
                {@const added = hasWindowContext(target.id)}
                <label class="window-option" class:added>
                  <input
                    type="checkbox"
                    checked={selectedWindowIds.has(target.id) || added}
                    disabled={added || pending || collectingWindows}
                    onchange={(event) =>
                      setWindowSelected(
                        target.id,
                        (event.currentTarget as HTMLInputElement).checked,
                      )}
                  />
                  <span title={windowLabel(target)}>{windowLabel(target)}</span>
                  <small>{added ? '已添加' : target.applicationName || target.processName}</small>
                </label>
              {/each}
            {/if}
          </div>
          {#if contextActionError}<p class="context-action-error">{contextActionError}</p>{/if}
          <button
            class="add-window-context"
            type="button"
            onclick={addSelectedWindows}
            disabled={selectedWindowIds.size === 0 || collectingWindows || pending}
          >
            {collectingWindows ? '正在读取所选窗口…' : `添加所选窗口（${selectedWindowIds.size}）`}
          </button>
        </div>
      {/if}
    </div>
    <span class="privacy-note">仅在主动选择后采集</span>
  </section>

  <section class="conversation" aria-live="polite" bind:this={conversationElement}>
    {#if bootstrapError}
      <p class="system-error">模型信息加载失败：{bootstrapError}</p>
    {/if}
    {#if contextWarning}
      <p class="system-status">窗口上下文暂不可用：{contextWarning}</p>
    {/if}
    {#if modelRestoreWarning}
      <p class="system-status">{modelRestoreWarning}</p>
    {/if}
    {#if historySaveError}
      <p class="system-error">历史对话保存失败：{historySaveError}</p>
    {/if}
    {#if contextResults.length > 0}
      <div class="context-results">
        {#each contextResults as result, index (`${result.source}-${index}`)}
          <small class:failed={result.status === 'failed'}>
            {contextSourceLabel(result.source)}：{result.status === 'added'
              ? `已添加 ${result.characterCount} 字${result.truncated ? '（已截断）' : ''}`
              : result.message}
          </small>
        {/each}
      </div>
    {/if}

    {#if messages.length === 0 && !responseState.content && responseState.status !== 'failed'}
      <div class="empty">
        <span>✦</span>
        <p>
          {activeProfile()?.providerType === 'mock'
            ? '当前使用本地 Mock Provider；仅在你选择上下文并发送时读取对应文字。'
            : `当前使用 ${activeProfile()?.name ?? '所选模型'}；仅在你选择上下文并发送时读取对应文字。`}
        </p>
      </div>
    {/if}

    {#each messages as message (message.id)}
      <article class:assistant-message={message.role === 'assistant'} class="message">
        <span>{message.role === 'user' ? '你' : 'DeskAide'}</span>
        <p>{message.content}</p>
        {#if message.note}<small>{message.note}</small>{/if}
      </article>
    {/each}

    {#if responseState.content}
      <article class="message assistant-message current-response">
        <span>DeskAide</span>
        <p>{responseState.content}</p>
        {#if responseState.status === 'streaming'}<i class="caret"></i>{/if}
        {#if responseState.status === 'cancelled'}<small>已停止</small>{/if}
        {#if responseState.status === 'failed'}<small>生成失败：{responseState.error}</small>{/if}
      </article>
    {:else if responseState.status === 'failed'}
      <p class="system-error">{responseState.error}</p>
    {:else if responseState.status === 'cancelled'}
      <p class="system-status">已停止生成</p>
    {/if}
  </section>

  {#if windowContexts.length > 0}
    <section class="context-drafts" aria-label="待发送的窗口上下文">
      {#each windowContexts as draft (draft.id)}
        <article class="context-card">
          <button
            class="context-card-main"
            type="button"
            title="打开内容编辑窗口"
            onclick={() => editWindowContext(draft)}
          >
            <strong>{windowLabel(draft.target)}</strong>
            <small>{contextExcerpt(draft.content)}</small>
          </button>
          <button
            class="context-card-remove"
            type="button"
            title="移除此上下文"
            aria-label={`移除 ${windowLabel(draft.target)}`}
            onclick={() => removeWindowContext(draft.id)}>×</button
          >
        </article>
      {/each}
    </section>
  {/if}

  <section class="composer">
    <textarea
      aria-label="问题"
      bind:this={textarea}
      bind:value={prompt}
      rows="3"
      placeholder="现在需要我帮你做什么？"
      disabled={pending}></textarea>
    <div class="composer-footer">
      <span>Ctrl + Enter 发送 · Esc 隐藏</span>
      {#if pending}
        <button class="stop" type="button" onclick={stop} disabled={stopping}>
          {stopping ? '正在停止' : '停止生成'}
        </button>
      {:else}
        <button
          class="send"
          type="button"
          onclick={submit}
          disabled={!prompt.trim() || !activeModelProfileId}>发送</button
        >
      {/if}
    </div>
  </section>

  {#if settingsOpen && bootstrap}
    <ModelSettings
      profiles={bootstrap.modelProfiles}
      activeProfileId={activeModelProfileId}
      {avatarPackId}
      {theme}
      onchanged={loadBootstrap}
      onavatarchange={changeAvatar}
      onthemechange={changeTheme}
      onclose={() => (settingsOpen = false)}
    />
  {/if}

  {#if historyOpen}
    <HistoryDrawer
      activeConversationId={conversationId}
      onselect={selectHistoryConversation}
      ondelete={deleteHistoryConversation}
      onclose={() => (historyOpen = false)}
    />
  {/if}
</main>

<style>
  .panel {
    position: relative;
    display: grid;
    width: 100%;
    height: 100%;
    padding: 16px;
    overflow: hidden;
    border: 1px solid var(--theme-border-strong);
    border-radius: 20px;
    grid-template-rows: auto auto minmax(0, 1fr) auto auto;
    gap: 11px;
    color: var(--theme-text);
    background: var(--theme-panel-background);
    box-shadow: var(--theme-shadow);
  }

  .panel.expanded {
    padding: 20px;
    gap: 14px;
  }

  header,
  .header-actions,
  .toolbar,
  .composer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .eyebrow {
    margin: 0 0 2px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.16em;
  }

  h1 {
    margin: 0;
    font-size: 19px;
  }

  .header-actions {
    gap: 6px;
  }

  .icon-button {
    display: grid;
    width: 29px;
    height: 29px;
    padding: 0;
    place-items: center;
    border: 0;
    border-radius: 9px;
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
    font-size: 17px;
    cursor: pointer;
  }

  .icon-button:hover {
    color: var(--theme-text-strong);
    background: var(--theme-control-hover);
  }

  .icon-button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .icon-button.pinned {
    color: var(--theme-accent);
    background: var(--theme-accent-soft);
  }

  .close {
    padding-bottom: 2px;
    font-size: 21px;
  }

  .toolbar {
    position: relative;
    justify-content: flex-start;
    gap: 8px;
  }

  .model-picker,
  .context-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 29px;
    padding: 4px 8px;
    border: 1px solid var(--theme-accent-border);
    border-radius: 9px;
    color: var(--theme-muted-strong);
    background: var(--theme-accent-soft);
    font-size: 10px;
  }

  .model-picker select {
    max-width: 120px;
    border: 0;
    outline: 0;
    color: var(--theme-accent-text);
    background: transparent;
    font-size: 11px;
  }

  .model-picker option {
    color: var(--theme-text);
    background: var(--theme-option-bg);
  }

  .context-picker {
    position: relative;
  }

  .context-trigger {
    color: var(--theme-accent-text);
    cursor: pointer;
  }

  .context-trigger > span {
    display: grid;
    width: 16px;
    height: 16px;
    place-items: center;
    border-radius: 99px;
    color: var(--theme-muted);
    background: var(--theme-control-bg);
  }

  .privacy-note {
    margin-left: auto;
    color: var(--theme-muted);
    font-size: 9px;
  }

  .context-menu {
    position: absolute;
    z-index: 5;
    top: 35px;
    left: 0;
    width: 340px;
    max-height: min(480px, calc(100vh - 90px));
    padding: 10px;
    overflow: auto;
    border: 1px solid var(--theme-border-strong);
    border-radius: 12px;
    background: var(--theme-popup-bg);
    box-shadow: var(--theme-popup-shadow);
  }

  .context-heading {
    display: flex;
    margin: 2px 2px 8px;
    flex-direction: column;
    gap: 2px;
  }

  .context-heading strong {
    font-size: 12px;
  }

  .context-heading small,
  .context-option small {
    color: var(--theme-muted);
    font-size: 9px;
  }

  .context-option {
    display: grid;
    padding: 7px 3px;
    grid-template-columns: 18px 1fr;
    column-gap: 4px;
    opacity: 0.62;
  }

  .context-option.available {
    opacity: 1;
    cursor: pointer;
  }

  .context-option input {
    grid-row: 1 / 3;
  }

  .context-option > span {
    font-size: 11px;
  }

  .window-list-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 8px 3px 5px;
    padding-top: 8px;
    border-top: 1px solid var(--theme-border);
    color: var(--theme-muted-strong);
    font-size: 10px;
    font-weight: 700;
  }

  .window-list-heading button {
    padding: 2px 6px;
    border: 0;
    border-radius: 6px;
    color: var(--theme-accent-text);
    background: var(--theme-control-bg);
    font-size: 9px;
    cursor: pointer;
  }

  .window-list {
    display: grid;
    max-height: 210px;
    overflow: auto;
    gap: 2px;
  }

  .window-option {
    display: grid;
    padding: 6px 3px;
    grid-template-columns: 18px minmax(0, 1fr);
    column-gap: 4px;
    cursor: pointer;
  }

  .window-option.added {
    opacity: 0.55;
    cursor: default;
  }

  .window-option input {
    grid-row: 1 / 3;
  }

  .window-option span,
  .window-option small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .window-option span {
    font-size: 11px;
  }

  .window-option small,
  .window-placeholder {
    color: var(--theme-muted);
    font-size: 9px;
  }

  .window-placeholder {
    padding: 12px 4px;
  }

  .context-action-error {
    margin: 7px 3px;
    color: var(--theme-error);
    font-size: 9px;
    line-height: 1.4;
  }

  .add-window-context {
    width: 100%;
    margin-top: 8px;
    padding: 7px 10px;
    border: 0;
    border-radius: 8px;
    color: var(--theme-primary-text);
    background: var(--theme-primary-background);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
  }

  .add-window-context:disabled {
    cursor: default;
    filter: saturate(0.35);
    opacity: 0.5;
  }

  .conversation {
    padding: 10px;
    overflow: auto;
    border: 1px solid var(--theme-border);
    border-radius: 13px;
    background: var(--theme-conversation-bg);
    scrollbar-color: var(--theme-scrollbar) transparent;
  }

  .empty {
    display: grid;
    height: 100%;
    place-content: center;
    color: var(--theme-muted);
    text-align: center;
  }

  .empty > span {
    color: var(--theme-accent);
    font-size: 21px;
  }

  .empty p {
    max-width: 285px;
    margin: 7px 0 0;
    font-size: 11px;
    line-height: 1.5;
  }

  .message {
    max-width: 88%;
    margin: 0 0 10px auto;
    padding: 9px 11px;
    border-radius: 12px 12px 3px 12px;
    background: var(--theme-message-user-bg);
  }

  .message.assistant-message {
    margin-right: auto;
    margin-left: 0;
    border-radius: 12px 12px 12px 3px;
    background: var(--theme-message-assistant-bg);
  }

  .message > span {
    display: block;
    margin-bottom: 3px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 700;
  }

  .message > p {
    display: inline;
    margin: 0;
    white-space: pre-wrap;
    font-size: 12px;
    line-height: 1.55;
  }

  .message > small {
    display: block;
    margin-top: 5px;
    color: var(--theme-muted);
    font-size: 9px;
  }

  .caret {
    display: inline-block;
    width: 6px;
    height: 14px;
    margin-left: 3px;
    vertical-align: -2px;
    background: var(--theme-accent);
    animation: blink 700ms steps(2) infinite;
  }

  .system-error {
    margin: 5px;
    color: var(--theme-error);
    font-size: 11px;
    line-height: 1.5;
  }

  .system-status {
    margin: 5px;
    color: var(--theme-muted-strong);
    font-size: 11px;
  }

  .context-results {
    display: grid;
    margin: 0 0 9px;
    padding: 7px 9px;
    gap: 3px;
    border-radius: 9px;
    color: var(--theme-muted-strong);
    background: var(--theme-accent-soft);
  }

  .context-results small {
    font-size: 9px;
    line-height: 1.4;
  }

  .context-results small.failed {
    color: var(--theme-error);
  }

  .context-drafts {
    display: flex;
    min-width: 0;
    padding: 1px 0;
    overflow-x: auto;
    gap: 7px;
    scrollbar-width: thin;
  }

  .context-card {
    display: flex;
    min-width: 180px;
    max-width: 265px;
    border: 1px solid var(--theme-accent-border);
    border-radius: 10px;
    background: var(--theme-accent-soft);
  }

  .context-card-main {
    display: grid;
    min-width: 0;
    padding: 7px 8px;
    flex: 1;
    border: 0;
    color: var(--theme-text);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .context-card-main strong,
  .context-card-main small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .context-card-main strong {
    font-size: 10px;
  }

  .context-card-main small {
    margin-top: 2px;
    color: var(--theme-muted);
    font-size: 8px;
  }

  .context-card-remove {
    width: 27px;
    padding: 0;
    border: 0;
    border-left: 1px solid var(--theme-border);
    color: var(--theme-muted);
    background: transparent;
    font-size: 16px;
    cursor: pointer;
  }

  .composer {
    padding: 10px;
    border: 1px solid var(--theme-border);
    border-radius: 13px;
    background: var(--theme-surface-bg);
  }

  textarea {
    width: 100%;
    min-height: 47px;
    padding: 0;
    resize: none;
    border: 0;
    outline: 0;
    color: var(--theme-text-strong);
    background: transparent;
    font-size: 12px;
    line-height: 1.5;
  }

  textarea::placeholder {
    color: var(--theme-muted);
  }

  .composer-footer {
    margin-top: 6px;
  }

  .composer-footer > span {
    color: var(--theme-muted);
    font-size: 9px;
  }

  .send,
  .stop {
    min-width: 68px;
    padding: 7px 12px;
    border: 0;
    border-radius: 8px;
    color: var(--theme-primary-text);
    background: var(--theme-primary-background);
    font-size: 11px;
    font-weight: 750;
    cursor: pointer;
  }

  .stop {
    color: var(--theme-stop-text);
    background: var(--theme-stop-background);
    box-shadow: inset 0 0 0 1px var(--theme-stop-border);
  }

  .send:disabled,
  .stop:disabled {
    cursor: default;
    filter: saturate(0.35);
    opacity: 0.5;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }
</style>
