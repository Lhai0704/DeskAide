<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import ModelSettings from '../settings/ModelSettings.svelte';
  import { buildModelMessages, type ConversationMessage } from './conversation';
  import {
    initialResponseState,
    reduceResponseEvent,
    type ResponseEvent,
    type ResponseState,
  } from './events';
  import { CONTEXT_OPTIONS, contextUnavailableReason, type AssistantBootstrap } from './model';

  let prompt = '';
  let responseState: ResponseState = initialResponseState();
  let messages: ConversationMessage[] = [];
  let pending = false;
  let stopping = false;
  let expanded = false;
  let contextOpen = false;
  let settingsOpen = false;
  let bootstrap: AssistantBootstrap | null = null;
  let activeModelProfileId = '';
  let conversationId = createId();
  let bootstrapError = '';
  let textarea: HTMLTextAreaElement;
  const ignoredRequestIds = new SvelteSet<string>();

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
      responseState = reduceResponseEvent(responseState, payload);
      if (
        payload.type === 'completed' ||
        payload.type === 'failed' ||
        payload.type === 'cancelled'
      ) {
        pending = false;
        stopping = false;
      }
    });
    const unlistenShown = listen('assistant-shown', () => {
      window.setTimeout(() => textarea?.focus(), 0);
    });

    void loadBootstrap();
    textarea?.focus();

    return () => {
      void unlistenResponse.then((unlisten) => unlisten());
      void unlistenShown.then((unlisten) => unlisten());
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

  function archiveCurrentResponse() {
    if (!responseState.content) return;
    messages = [
      ...messages,
      {
        id: createId(),
        role: 'assistant',
        content: responseState.content,
        note:
          responseState.status === 'cancelled'
            ? '已停止'
            : responseState.status === 'failed'
              ? `生成失败：${responseState.error}`
              : undefined,
      },
    ];
  }

  async function submit() {
    const value = prompt.trim();
    if (!value || pending || !activeModelProfileId) return;

    archiveCurrentResponse();
    messages = [...messages, { id: createId(), role: 'user', content: value }];
    prompt = '';
    pending = true;
    responseState = initialResponseState();
    try {
      const requestId = await invoke<string>('submit_model_request', {
        conversationId,
        messages: buildModelMessages(messages),
      });
      if (!responseState.requestId) responseState = { ...responseState, requestId };
    } catch (cause) {
      pending = false;
      responseState = {
        requestId: null,
        content: '',
        status: 'failed',
        error: cause instanceof Error ? cause.message : String(cause),
      };
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
    if (pending && responseState.requestId) {
      ignoredRequestIds.add(responseState.requestId);
      await stop();
    }
    messages = [];
    responseState = initialResponseState();
    prompt = '';
    conversationId = createId();
    window.setTimeout(() => textarea?.focus(), 0);
  }

  async function toggleExpanded() {
    const next = !expanded;
    await invoke('set_assistant_expanded', { expanded: next });
    expanded = next;
  }

  async function openSettings() {
    if (!expanded) {
      await invoke('set_assistant_expanded', { expanded: true });
      expanded = true;
    }
    contextOpen = false;
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

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (settingsOpen) {
        settingsOpen = false;
        return;
      }
      contextOpen = false;
      void invoke('hide_assistant');
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void submit();
    }
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
      <button class="icon-button" type="button" title="新建会话" onclick={newConversation}
        >＋</button
      >
      <button class="icon-button" type="button" title="模型设置" onclick={openSettings}>⚙</button>
      <button
        class="icon-button"
        type="button"
        title={expanded ? '收起窗口' : '展开窗口'}
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
        onclick={() => (contextOpen = !contextOpen)}
      >
        上下文 <span>0</span>
      </button>
      {#if contextOpen && activeProfile()}
        <div class="context-menu">
          <div class="context-heading">
            <strong>添加本次上下文</strong>
            <small>当前阶段尚未启用采集</small>
          </div>
          {#each CONTEXT_OPTIONS as option (option.id)}
            <label
              class="context-option"
              title={contextUnavailableReason(option, activeProfile()!.capabilities)}
            >
              <input type="checkbox" disabled />
              <span>{option.label}</span>
              <small>{contextUnavailableReason(option, activeProfile()!.capabilities)}</small>
            </label>
          {/each}
        </div>
      {/if}
    </div>
    <span class="privacy-note">仅在主动选择后采集</span>
  </section>

  <section class="conversation" aria-live="polite">
    {#if bootstrapError}
      <p class="system-error">模型信息加载失败：{bootstrapError}</p>
    {/if}

    {#if messages.length === 0 && !responseState.content && responseState.status !== 'failed'}
      <div class="empty">
        <span>✦</span>
        <p>
          {activeProfile()?.providerType === 'mock'
            ? '当前使用本地 Mock Provider，不会发送网络请求或读取电脑内容。'
            : `当前使用 ${activeProfile()?.name ?? '所选模型'}；仅发送本会话文字历史，不读取电脑内容。`}
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
      onchanged={loadBootstrap}
      onclose={() => (settingsOpen = false)}
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
    border: 1px solid rgb(151 198 255 / 22%);
    border-radius: 20px;
    grid-template-rows: auto auto minmax(0, 1fr) auto;
    gap: 11px;
    color: #eaf2ff;
    background:
      radial-gradient(circle at 92% 0%, rgb(53 143 196 / 24%), transparent 38%),
      linear-gradient(145deg, rgb(19 27 42 / 98%), rgb(9 14 24 / 98%));
    box-shadow: 0 18px 60px rgb(0 0 0 / 40%);
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
    color: #7ee2ff;
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
    color: #aebbd0;
    background: rgb(255 255 255 / 6%);
    font-size: 17px;
    cursor: pointer;
  }

  .icon-button:hover {
    color: #eff8ff;
    background: rgb(255 255 255 / 11%);
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
    border: 1px solid rgb(126 226 255 / 17%);
    border-radius: 9px;
    color: #9fb0c8;
    background: rgb(126 226 255 / 6%);
    font-size: 10px;
  }

  .model-picker select {
    max-width: 120px;
    border: 0;
    outline: 0;
    color: #c9f3ff;
    background: transparent;
    font-size: 11px;
  }

  .model-picker option {
    color: #eaf2ff;
    background: #111b2a;
  }

  .context-picker {
    position: relative;
  }

  .context-trigger {
    color: #c9f3ff;
    cursor: pointer;
  }

  .context-trigger > span {
    display: grid;
    width: 16px;
    height: 16px;
    place-items: center;
    border-radius: 99px;
    color: #8da0bb;
    background: rgb(255 255 255 / 8%);
  }

  .privacy-note {
    margin-left: auto;
    color: #63748d;
    font-size: 9px;
  }

  .context-menu {
    position: absolute;
    z-index: 5;
    top: 35px;
    left: 0;
    width: 285px;
    padding: 10px;
    border: 1px solid rgb(151 198 255 / 20%);
    border-radius: 12px;
    background: rgb(12 20 32 / 99%);
    box-shadow: 0 14px 36px rgb(0 0 0 / 45%);
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
    color: #687991;
    font-size: 9px;
  }

  .context-option {
    display: grid;
    padding: 7px 3px;
    grid-template-columns: 18px 1fr;
    column-gap: 4px;
    opacity: 0.62;
  }

  .context-option input {
    grid-row: 1 / 3;
  }

  .context-option > span {
    font-size: 11px;
  }

  .conversation {
    padding: 10px;
    overflow: auto;
    border: 1px solid rgb(255 255 255 / 6%);
    border-radius: 13px;
    background: rgb(3 8 15 / 45%);
    scrollbar-color: #304158 transparent;
  }

  .empty {
    display: grid;
    height: 100%;
    place-content: center;
    color: #7889a3;
    text-align: center;
  }

  .empty > span {
    color: #7ee2ff;
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
    background: rgb(77 144 208 / 18%);
  }

  .message.assistant-message {
    margin-right: auto;
    margin-left: 0;
    border-radius: 12px 12px 12px 3px;
    background: rgb(255 255 255 / 5%);
  }

  .message > span {
    display: block;
    margin-bottom: 3px;
    color: #7ee2ff;
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
    color: #8291a6;
    font-size: 9px;
  }

  .caret {
    display: inline-block;
    width: 6px;
    height: 14px;
    margin-left: 3px;
    vertical-align: -2px;
    background: #7ee2ff;
    animation: blink 700ms steps(2) infinite;
  }

  .system-error {
    margin: 5px;
    color: #ff9f9f;
    font-size: 11px;
    line-height: 1.5;
  }

  .system-status {
    margin: 5px;
    color: #93a7c0;
    font-size: 11px;
  }

  .composer {
    padding: 10px;
    border: 1px solid rgb(255 255 255 / 9%);
    border-radius: 13px;
    background: rgb(255 255 255 / 4%);
  }

  textarea {
    width: 100%;
    min-height: 47px;
    padding: 0;
    resize: none;
    border: 0;
    outline: 0;
    color: #f4f8ff;
    background: transparent;
    font-size: 12px;
    line-height: 1.5;
  }

  textarea::placeholder {
    color: #71819a;
  }

  .composer-footer {
    margin-top: 6px;
  }

  .composer-footer > span {
    color: #667790;
    font-size: 9px;
  }

  .send,
  .stop {
    min-width: 68px;
    padding: 7px 12px;
    border: 0;
    border-radius: 8px;
    color: #071019;
    background: linear-gradient(135deg, #7ee2ff, #79b7ff);
    font-size: 11px;
    font-weight: 750;
    cursor: pointer;
  }

  .stop {
    color: #ffcece;
    background: rgb(255 102 102 / 14%);
    box-shadow: inset 0 0 0 1px rgb(255 139 139 / 18%);
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
