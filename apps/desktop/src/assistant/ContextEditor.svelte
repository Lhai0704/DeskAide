<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { contextDraftLabel, contextDraftMeta, type TextContextDraft } from './model';

  let draft: TextContextDraft | null = null;
  let content = '';
  let saving = false;
  let error = '';
  let textarea: HTMLTextAreaElement;

  onMount(() => {
    const applyDraft = (next: TextContextDraft | null) => {
      draft = next;
      content = next?.content ?? '';
      error = '';
      window.setTimeout(() => textarea?.focus(), 0);
    };
    const unlistenOpened = listen<TextContextDraft>('context-editor-opened', ({ payload }) => {
      applyDraft(payload);
    });
    const unlistenClose = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      void close();
    });
    void invoke<TextContextDraft | null>('get_context_editor_draft').then(applyDraft);

    return () => {
      void unlistenOpened.then((unlisten) => unlisten());
      void unlistenClose.then((unlisten) => unlisten());
    };
  });

  async function save() {
    if (!draft || !content.trim() || saving) return;
    saving = true;
    error = '';
    try {
      await invoke('save_context_editor_draft', {
        draft: { ...draft, content },
      });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  async function close() {
    error = '';
    try {
      await invoke('close_context_editor');
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      void close();
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<main class="editor-shell">
  <header>
    <div>
      <p>文字上下文</p>
      <h1>{draft ? contextDraftLabel(draft) : '正在加载…'}</h1>
    </div>
    <button type="button" aria-label="关闭" onclick={close}>×</button>
  </header>

  {#if draft}
    <div class="source-meta">
      <span>{contextDraftMeta(draft)}</span>
      <span>{content.length.toLocaleString()} 字符</span>
    </div>
    <label>
      <span>将要加入本次提问的内容</span>
      <textarea bind:this={textarea} bind:value={content} spellcheck="false"></textarea>
    </label>
  {:else}
    <p class="empty">没有可编辑的上下文草稿。</p>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
  <footer>
    <span>Ctrl + Enter 保存 · Esc 关闭</span>
    <div>
      <button class="secondary" type="button" onclick={close}>取消</button>
      <button
        class="primary"
        type="button"
        onclick={save}
        disabled={!draft || !content.trim() || saving}
      >
        {saving ? '正在保存…' : '保存修改'}
      </button>
    </div>
  </footer>
</main>

<style>
  .editor-shell {
    display: grid;
    width: 100%;
    height: 100%;
    padding: 22px;
    grid-template-rows: auto auto minmax(0, 1fr) auto auto;
    gap: 13px;
    color: var(--theme-text);
    background: var(--theme-settings-background);
  }

  header,
  footer,
  .source-meta,
  footer > div {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  header p {
    margin: 0 0 3px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.14em;
  }

  h1 {
    max-width: 510px;
    margin: 0;
    overflow: hidden;
    font-size: 17px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header button {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 8px;
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
    font-size: 20px;
    cursor: pointer;
  }

  .source-meta {
    color: var(--theme-muted);
    font-size: 10px;
  }

  label {
    display: grid;
    min-height: 0;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 7px;
  }

  label > span {
    color: var(--theme-muted-strong);
    font-size: 10px;
    font-weight: 700;
  }

  textarea {
    width: 100%;
    height: 100%;
    min-height: 250px;
    padding: 13px;
    resize: none;
    border: 1px solid var(--theme-border-strong);
    border-radius: 12px;
    outline: 0;
    color: var(--theme-text-strong);
    background: var(--theme-input-bg);
    font-family: 'Cascadia Code', Consolas, 'Microsoft YaHei', monospace;
    font-size: 11px;
    line-height: 1.55;
    scrollbar-color: var(--theme-scrollbar) transparent;
  }

  textarea:focus {
    border-color: var(--theme-accent-border);
    box-shadow: 0 0 0 2px var(--theme-accent-soft);
  }

  .error {
    margin: 0;
    color: var(--theme-error);
    font-size: 10px;
  }

  .empty {
    color: var(--theme-muted);
  }

  footer > span {
    color: var(--theme-muted);
    font-size: 9px;
  }

  footer > div {
    gap: 8px;
  }

  footer button {
    padding: 7px 13px;
    border: 0;
    border-radius: 8px;
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
  }

  .secondary {
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
  }

  .primary {
    color: var(--theme-primary-text);
    background: var(--theme-primary-background);
  }

  .primary:disabled {
    cursor: default;
    filter: saturate(0.35);
    opacity: 0.5;
  }
</style>
