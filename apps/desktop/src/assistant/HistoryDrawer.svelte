<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { ConversationSummary } from './conversation';

  interface Props {
    activeConversationId: string;
    onselect: (conversationId: string) => Promise<void>;
    ondelete: (conversationId: string) => Promise<void>;
    onclose: () => void;
  }

  let { activeConversationId, onselect, ondelete, onclose }: Props = $props();
  let summaries = $state<ConversationSummary[]>([]);
  let loading = $state(true);
  let error = $state('');
  let busyId = $state('');
  let editingId = $state('');
  let editingTitle = $state('');
  let confirmingDeleteId = $state('');

  onMount(() => {
    void loadHistory();
  });

  async function loadHistory() {
    loading = true;
    error = '';
    try {
      summaries = await invoke<ConversationSummary[]>('list_conversation_summaries');
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  async function selectConversation(conversationId: string) {
    if (busyId) return;
    busyId = conversationId;
    error = '';
    try {
      await onselect(conversationId);
    } catch (cause) {
      error = errorMessage(cause);
      busyId = '';
    }
  }

  function beginRename(summary: ConversationSummary) {
    editingId = summary.id;
    editingTitle = summary.title;
    confirmingDeleteId = '';
  }

  function cancelRename() {
    editingId = '';
    editingTitle = '';
  }

  async function renameConversation() {
    const title = editingTitle.trim();
    if (!editingId || !title || busyId) return;
    const conversationId = editingId;
    busyId = conversationId;
    error = '';
    try {
      const updated = await invoke<ConversationSummary>('rename_conversation', {
        conversationId,
        title,
      });
      summaries = summaries
        .map((summary) => (summary.id === conversationId ? updated : summary))
        .sort((left, right) => right.updatedAtMs - left.updatedAtMs);
      cancelRename();
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      busyId = '';
    }
  }

  async function confirmDelete(conversationId: string) {
    if (busyId) return;
    busyId = conversationId;
    error = '';
    try {
      await ondelete(conversationId);
      summaries = summaries.filter((summary) => summary.id !== conversationId);
      confirmingDeleteId = '';
      if (editingId === conversationId) cancelRename();
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      busyId = '';
    }
  }

  function formatUpdatedAt(timestamp: number) {
    return new Intl.DateTimeFormat('zh-CN', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(timestamp));
  }

  function errorMessage(cause: unknown) {
    return cause instanceof Error ? cause.message : String(cause);
  }
</script>

<div class="history-layer">
  <button class="history-backdrop" type="button" aria-label="关闭历史对话" onclick={onclose}
  ></button>
  <dialog class="history-drawer" open aria-labelledby="history-title">
    <header>
      <div>
        <p>CONVERSATIONS</p>
        <h2 id="history-title">历史对话</h2>
      </div>
      <button class="close" type="button" aria-label="关闭历史对话" onclick={onclose}>×</button>
    </header>

    <div class="history-content">
      {#if loading}
        <p class="placeholder">正在加载历史对话…</p>
      {:else if error && summaries.length === 0}
        <div class="load-error">
          <p>{error}</p>
          <button type="button" onclick={loadHistory}>重试</button>
        </div>
      {:else if summaries.length === 0}
        <div class="empty-history">
          <span>◷</span>
          <strong>还没有历史对话</strong>
          <p>发送第一条消息后，对话会自动保存在这台电脑上。</p>
        </div>
      {:else}
        {#if error}<p class="inline-error">{error}</p>{/if}
        <div class="history-list">
          {#each summaries as summary (summary.id)}
            <article class:active={summary.id === activeConversationId} class="history-item">
              {#if editingId === summary.id}
                <form
                  class="rename-form"
                  onsubmit={(event) => {
                    event.preventDefault();
                    void renameConversation();
                  }}
                >
                  <input
                    aria-label="对话标题"
                    maxlength="80"
                    bind:value={editingTitle}
                    disabled={busyId === summary.id}
                  />
                  <div>
                    <button
                      class="primary-action"
                      type="submit"
                      disabled={!editingTitle.trim() || busyId === summary.id}>保存</button
                    >
                    <button type="button" onclick={cancelRename} disabled={busyId === summary.id}
                      >取消</button
                    >
                  </div>
                </form>
              {:else}
                <button
                  class="conversation-main"
                  type="button"
                  disabled={Boolean(busyId)}
                  onclick={() => selectConversation(summary.id)}
                >
                  <strong>{summary.title}</strong>
                  <small>
                    {formatUpdatedAt(summary.updatedAtMs)} · {summary.messageCount} 条消息
                  </small>
                </button>
                <div class="item-actions">
                  {#if confirmingDeleteId === summary.id}
                    <span>确认删除？</span>
                    <button
                      class="danger"
                      type="button"
                      disabled={busyId === summary.id}
                      onclick={() => confirmDelete(summary.id)}>删除</button
                    >
                    <button
                      type="button"
                      disabled={busyId === summary.id}
                      onclick={() => (confirmingDeleteId = '')}>取消</button
                    >
                  {:else}
                    <button type="button" onclick={() => beginRename(summary)}>重命名</button>
                    <button
                      class="danger-text"
                      type="button"
                      onclick={() => {
                        confirmingDeleteId = summary.id;
                        editingId = '';
                      }}>删除</button
                    >
                  {/if}
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
    </div>

    <footer>对话仅保存在本机；窗口上下文不会写入历史。</footer>
  </dialog>
</div>

<style>
  .history-layer {
    position: absolute;
    z-index: 9;
    inset: 0;
    display: flex;
    justify-content: flex-end;
    overflow: hidden;
    border-radius: 20px;
    background: rgb(0 0 0 / 32%);
  }

  .history-backdrop {
    position: absolute;
    inset: 0;
    padding: 0;
    border: 0;
    border-radius: 20px;
    background: transparent;
  }

  .history-backdrop:hover:not(:disabled) {
    background: transparent;
  }

  .history-drawer {
    position: relative;
    z-index: 1;
    display: grid;
    width: min(370px, 88%);
    min-width: 290px;
    height: 100%;
    margin: 0;
    margin-left: auto;
    padding: 0;
    grid-template-rows: auto minmax(0, 1fr) auto;
    border-left: 1px solid var(--theme-border-strong);
    color: var(--theme-text);
    background: var(--theme-settings-background);
    box-shadow: -16px 0 42px rgb(0 0 0 / 28%);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 18px 12px;
  }

  header p {
    margin: 0 0 2px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.16em;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  button {
    border: 0;
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    color: var(--theme-text-strong);
    background: var(--theme-control-hover);
  }

  button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .close {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    font-size: 20px;
  }

  .history-content {
    min-height: 0;
    padding: 4px 12px 12px;
    overflow: auto;
    scrollbar-color: var(--theme-scrollbar) transparent;
  }

  .history-list {
    display: grid;
    gap: 8px;
  }

  .history-item {
    display: grid;
    padding: 9px;
    gap: 7px;
    border: 1px solid var(--theme-border);
    border-radius: 11px;
    background: var(--theme-surface-bg);
  }

  .history-item.active {
    border-color: var(--theme-accent-border);
    background: var(--theme-accent-soft);
  }

  .conversation-main {
    display: grid;
    min-width: 0;
    padding: 2px;
    gap: 4px;
    color: var(--theme-text);
    background: transparent;
    text-align: left;
  }

  .conversation-main strong,
  .conversation-main small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conversation-main strong {
    font-size: 11px;
    font-weight: 700;
  }

  .conversation-main small {
    color: var(--theme-muted);
    font-size: 9px;
  }

  .item-actions,
  .rename-form > div {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 5px;
  }

  .item-actions button,
  .rename-form button,
  .load-error button {
    padding: 5px 7px;
    border-radius: 6px;
    font-size: 9px;
  }

  .item-actions span {
    margin-right: auto;
    color: var(--theme-danger);
    font-size: 9px;
  }

  .item-actions .danger,
  .item-actions .danger-text {
    color: var(--theme-danger);
  }

  .item-actions .danger {
    background: var(--theme-stop-background);
    box-shadow: inset 0 0 0 1px var(--theme-stop-border);
  }

  .rename-form {
    display: grid;
    gap: 7px;
  }

  .rename-form input {
    width: 100%;
    padding: 7px 8px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 7px;
    outline: 0;
    color: var(--theme-text-strong);
    background: var(--theme-input-bg);
    font-size: 11px;
  }

  .rename-form input:focus {
    border-color: var(--theme-accent);
  }

  .rename-form .primary-action {
    color: var(--theme-primary-text);
    background: var(--theme-primary-background);
  }

  .placeholder,
  .inline-error,
  .load-error,
  .empty-history {
    margin: 8px;
    font-size: 10px;
    line-height: 1.5;
  }

  .placeholder {
    color: var(--theme-muted);
  }

  .inline-error,
  .load-error {
    color: var(--theme-error);
  }

  .inline-error {
    margin-bottom: 10px;
  }

  .load-error button {
    margin-top: 7px;
  }

  .empty-history {
    display: grid;
    height: 100%;
    place-content: center;
    color: var(--theme-muted);
    text-align: center;
  }

  .empty-history span {
    color: var(--theme-accent);
    font-size: 25px;
  }

  .empty-history strong {
    margin-top: 8px;
    color: var(--theme-muted-strong);
    font-size: 12px;
  }

  .empty-history p {
    max-width: 220px;
    margin: 5px 0 0;
  }

  footer {
    padding: 10px 18px 14px;
    border-top: 1px solid var(--theme-border);
    color: var(--theme-muted);
    font-size: 8px;
    line-height: 1.45;
  }
</style>
