<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import {
    initialResponseState,
    reduceResponseEvent,
    type ResponseEvent,
    type ResponseState,
  } from './events';

  let prompt = '';
  let responseState: ResponseState = initialResponseState();
  let pending = false;
  let textarea: HTMLTextAreaElement;

  onMount(() => {
    const unlistenResponse = listen<ResponseEvent>('model-response', ({ payload }) => {
      responseState = reduceResponseEvent(responseState, payload);
      if (payload.type === 'completed' || payload.type === 'failed') pending = false;
    });
    const unlistenShown = listen('assistant-shown', () => {
      window.setTimeout(() => textarea?.focus(), 0);
    });
    textarea?.focus();

    return () => {
      void unlistenResponse.then((unlisten) => unlisten());
      void unlistenShown.then((unlisten) => unlisten());
    };
  });

  async function submit() {
    const value = prompt.trim();
    if (!value || pending) return;

    pending = true;
    responseState = initialResponseState();
    try {
      const requestId = await invoke<string>('submit_mock_request', { prompt: value });
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

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      void invoke('hide_assistant');
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void submit();
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<main class="panel">
  <header>
    <div>
      <p class="eyebrow">DESKTOP ASSISTANT</p>
      <h1>DeskAide</h1>
    </div>
    <div class="header-actions">
      <span class="model">Mock · Local</span>
      <button
        class="close"
        type="button"
        aria-label="隐藏助手"
        onclick={() => invoke('hide_assistant')}
      >
        ×
      </button>
    </div>
  </header>

  <section class="composer">
    <label for="prompt">现在需要我帮你做什么？</label>
    <textarea
      id="prompt"
      bind:this={textarea}
      bind:value={prompt}
      rows="4"
      placeholder="输入一个问题……"
      disabled={pending}></textarea>
    <div class="composer-footer">
      <span>Ctrl + Enter 发送 · Esc 隐藏</span>
      <button class="send" type="button" onclick={submit} disabled={pending || !prompt.trim()}>
        {pending ? '思考中…' : '发送'}
      </button>
    </div>
  </section>

  <section class="answer" aria-live="polite">
    {#if responseState.content}
      <p>{responseState.content}</p>
      {#if responseState.status === 'streaming'}<span class="caret"></span>{/if}
    {:else if responseState.status === 'failed'}
      <p class="error">{responseState.error}</p>
    {:else}
      <div class="empty">
        <span>✦</span>
        <p>第一阶段使用本地 Mock Provider，不会发送任何网络请求。</p>
      </div>
    {/if}
  </section>
</main>

<style>
  .panel {
    display: grid;
    width: 100%;
    height: 100%;
    padding: 18px;
    overflow: hidden;
    border: 1px solid rgb(151 198 255 / 22%);
    border-radius: 20px;
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: 14px;
    color: #eaf2ff;
    background:
      radial-gradient(circle at 92% 0%, rgb(53 143 196 / 24%), transparent 38%),
      linear-gradient(145deg, rgb(19 27 42 / 98%), rgb(9 14 24 / 98%));
    box-shadow: 0 18px 60px rgb(0 0 0 / 40%);
  }

  header,
  .header-actions,
  .composer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .eyebrow {
    margin: 0 0 2px;
    color: #7ee2ff;
    font-size: 10px;
    font-weight: 750;
    letter-spacing: 0.16em;
  }

  h1 {
    margin: 0;
    font-size: 20px;
  }

  .header-actions {
    gap: 10px;
  }

  .model {
    padding: 5px 9px;
    border: 1px solid rgb(126 226 255 / 22%);
    border-radius: 999px;
    color: #bfefff;
    background: rgb(126 226 255 / 8%);
    font-size: 11px;
  }

  .close {
    width: 30px;
    height: 30px;
    padding: 0 0 2px;
    border: 0;
    border-radius: 9px;
    color: #aebbd0;
    background: rgb(255 255 255 / 6%);
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
  }

  .composer {
    padding: 12px;
    border: 1px solid rgb(255 255 255 / 8%);
    border-radius: 14px;
    background: rgb(255 255 255 / 4%);
  }

  label {
    display: block;
    margin-bottom: 8px;
    color: #dce9fa;
    font-size: 13px;
    font-weight: 650;
  }

  textarea {
    width: 100%;
    min-height: 72px;
    padding: 0;
    resize: none;
    border: 0;
    outline: 0;
    color: #f4f8ff;
    background: transparent;
    line-height: 1.55;
  }

  textarea::placeholder {
    color: #71819a;
  }

  .composer-footer {
    margin-top: 8px;
  }

  .composer-footer > span {
    color: #71819a;
    font-size: 10px;
  }

  .send {
    padding: 7px 15px;
    border: 0;
    border-radius: 9px;
    color: #071019;
    background: linear-gradient(135deg, #7ee2ff, #79b7ff);
    font-size: 12px;
    font-weight: 750;
    cursor: pointer;
  }

  .send:disabled {
    cursor: default;
    filter: saturate(0.35);
    opacity: 0.5;
  }

  .answer {
    padding: 14px;
    overflow: auto;
    border: 1px solid rgb(255 255 255 / 6%);
    border-radius: 14px;
    background: rgb(3 8 15 / 45%);
  }

  .answer > p {
    display: inline;
    margin: 0;
    white-space: pre-wrap;
    line-height: 1.65;
  }

  .caret {
    display: inline-block;
    width: 7px;
    height: 16px;
    margin-left: 3px;
    vertical-align: -3px;
    background: #7ee2ff;
    animation: blink 700ms steps(2) infinite;
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
    font-size: 22px;
  }

  .empty p {
    max-width: 270px;
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 1.5;
  }

  .error {
    color: #ff9f9f;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }
</style>
