<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { SpeechSettings } from '../speech/controller';
  interface Reference {
    file: string;
    name: string;
    text: string;
  }
  let {
    settings,
    onchange,
    onpreview,
    onstop,
    status,
    active,
  }: {
    settings: SpeechSettings;
    onchange: (settings: SpeechSettings) => Promise<void>;
    onpreview: (settings: SpeechSettings) => void;
    onstop: () => void;
    status: string;
    active: boolean;
  } = $props();
  function initialSettings() {
    return { ...settings };
  }
  let draft = $state<SpeechSettings>(initialSettings());
  let references = $state<Reference[]>([]);
  let message = $state('尚未连接');
  let busy = $state(false);
  async function connect() {
    busy = true;
    message = '正在启动 / 连接本地服务…';
    try {
      const config = { ...draft, enabled: false };
      const service = await invoke<{
        ready: boolean;
        active_model: string | null;
        task: { busy: boolean };
      }>('check_speech_service', { settings: config });
      const library = await invoke<{ references: Reference[] }>('speech_references', {
        settings: config,
      });
      references = library.references;
      message = service.task.busy
        ? '服务正在处理其他任务'
        : service.ready
          ? `已连接 · ${service.active_model} 已加载`
          : '已连接 · 首次播报将加载模型';
      if (!references.length) message += ' · 请先在 TTS 网页添加参考声音';
    } catch (e) {
      message = String(e);
    } finally {
      busy = false;
    }
  }
  async function save() {
    try {
      await onchange({ ...draft });
      message = '语音设置已保存';
    } catch (e) {
      message = String(e);
    }
  }
</script>

<section class="speech-settings">
  <div>
    <p class="eyebrow">VOICE</p>
    <h3>语音播报</h3>
    <p>让助手边回复边说话，声音由本机 Qwen3-TTS 生成。</p>
  </div>
  <label class="toggle"
    ><input type="checkbox" bind:checked={draft.enabled} /> 自动朗读助手回复</label
  >
  <label
    >音量 · {Math.round(draft.volume * 100)}%<input
      aria-label="播报音量"
      type="range"
      min="0"
      max="1"
      step="0.05"
      bind:value={draft.volume}
    /></label
  >
  <label>项目目录<input bind:value={draft.projectDir} spellcheck="false" /></label>
  <button type="button" disabled={busy} onclick={connect}
    >{busy ? '连接中…' : '测试连接 / 刷新声音'}</button
  >
  <p class="notice" role="status">{message}</p>
  <label
    >参考声音<select bind:value={draft.reference}>
      <option value="">请选择已有参考声音</option>
      {#if draft.reference && !references.some((r) => r.file === draft.reference)}<option
          value={draft.reference}>{draft.reference}（待连接确认）</option
        >{/if}
      {#each references as reference (reference.file)}<option value={reference.file}
          >{reference.name}</option
        >{/each}
    </select></label
  >
  <label
    >语音模型<select bind:value={draft.model}
      ><option value="0.6B">0.6B · 默认</option><option value="1.7B">1.7B</option></select
    ></label
  >
  <p class="hint">
    使用素材库对应的参考文字。声音与模型设置从下一轮回复生效；播报不保存到作品库。面板隐藏后继续播放。
  </p>
  <div class="actions">
    <button
      type="button"
      disabled={!draft.reference || busy}
      onclick={() => onpreview({ ...draft })}>试听</button
    ><button type="button" disabled={!active} onclick={onstop}>停止朗读</button><button
      class="primary"
      type="button"
      onclick={save}>保存设置</button
    >
  </div>
  {#if status}<p class="notice" role="status">{status}</p>{/if}
</section>

<style>
  .speech-settings {
    display: grid;
    gap: 14px;
    color: var(--theme-text);
    max-width: 520px;
  }
  h3 {
    margin: 3px 0 8px;
    font-size: 20px;
  }
  p {
    margin: 0;
    font-size: 12px;
    line-height: 1.6;
    opacity: 0.8;
  }
  .eyebrow {
    color: var(--theme-accent);
    letter-spacing: 0.16em;
    font-size: 10px;
  }
  label {
    display: grid;
    gap: 7px;
    font-size: 13px;
  }
  .toggle {
    display: flex;
    align-items: center;
  }
  input:not([type='checkbox']),
  select {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  input:not([type='checkbox']):not([type='range']),
  select,
  button {
    border: 1px solid var(--theme-border-strong);
    border-radius: 8px;
    padding: 9px 10px;
    color: var(--theme-text);
    background: var(--theme-settings-background);
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .primary {
    border-color: var(--theme-accent);
  }
  button {
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .notice {
    overflow-wrap: anywhere;
  }
</style>
