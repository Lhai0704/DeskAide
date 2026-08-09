<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { ModelProfile } from '../assistant/model';
  import { canDeleteProfile } from './profileState';
  import {
    newProfileDraft,
    profileToDraft,
    toProfilePayload,
    validateModelProfile,
  } from './validation';

  interface Props {
    profile: ModelProfile | null;
    activeProfileId: string;
    onsaved: (profile: ModelProfile) => void;
    ondeleted: (profileId: string) => void;
  }

  let { profile, activeProfileId, onsaved, ondeleted }: Props = $props();
  function initialDraft() {
    return profile ? profileToDraft(profile) : newProfileDraft();
  }
  let draft = $state(initialDraft());
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');
  let validationErrors = $derived(validateModelProfile(draft));
  let readonly = $derived(profile?.providerType === 'mock');

  async function save() {
    error = '';
    notice = '';
    if (validationErrors.length) {
      error = validationErrors[0];
      return;
    }
    busy = true;
    try {
      const saved = await invoke<ModelProfile>('save_model_profile', {
        profile: toProfilePayload(draft),
      });
      draft = profileToDraft(saved);
      notice = saved.hasApiKey ? '配置和 API Key 已安全保存' : '配置已保存；尚未设置 API Key';
      onsaved(saved);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function testConnection() {
    if (!draft.id) {
      error = '请先保存 Profile，再测试连接';
      return;
    }
    if (draft.apiKey.trim()) {
      error = 'API Key 有未保存的更改，请先保存';
      return;
    }
    busy = true;
    error = '';
    notice = '';
    try {
      notice = await invoke<string>('test_model_connection', { profileId: draft.id });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!profile || !canDeleteProfile(profile, activeProfileId)) return;
    if (!window.confirm(`删除模型配置“${profile.name}”？其安全存储中的 API Key 也会删除。`)) return;
    busy = true;
    error = '';
    try {
      await invoke('delete_model_profile', { profileId: profile.id });
      ondeleted(profile.id);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

{#if readonly && profile}
  <section class="readonly-profile">
    <h3>{profile.name}</h3>
    <p>内置离线 Mock Profile 始终保留，不能编辑或删除，也不需要 API Key。</p>
    <dl>
      <div>
        <dt>Model ID</dt>
        <dd>{profile.modelId}</dd>
      </div>
      <div>
        <dt>流式输出</dt>
        <dd>支持</dd>
      </div>
      <div>
        <dt>上下文窗口</dt>
        <dd>{profile.capabilities.contextWindow}</dd>
      </div>
    </dl>
    <button type="button" onclick={testConnection} disabled={busy}>测试本地 Provider</button>
  </section>
{:else}
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
  >
    <div class="field-grid">
      <label>
        <span>Profile 名称</span>
        <input bind:value={draft.name} placeholder="例如 LongCat" disabled={busy} />
      </label>
      <label>
        <span>Model ID</span>
        <input bind:value={draft.modelId} placeholder="例如 LongCat-2.0" disabled={busy} />
      </label>
    </div>
    <label>
      <span>Base URL</span>
      <input
        bind:value={draft.baseUrl}
        placeholder="https://api.example.com/v1"
        autocomplete="url"
        disabled={busy}
      />
      <small>可填写以 /v1 结尾或 Provider 根路径；DeskAide 会规范化请求地址。</small>
    </label>
    <label>
      <span>API Key · {profile?.hasApiKey ? '已设置' : '未设置'}</span>
      <input
        type="password"
        bind:value={draft.apiKey}
        placeholder={profile?.hasApiKey ? '留空则保持现有密钥' : '输入后保存到系统凭据库'}
        autocomplete="new-password"
        disabled={busy}
      />
      <small>密钥不会回填到界面，也不会保存到 Tauri Store。</small>
    </label>

    <div class="checks">
      <label
        ><input type="checkbox" bind:checked={draft.capabilities.supportsStreaming} /> 支持流式输出</label
      >
      <label
        ><input type="checkbox" bind:checked={draft.capabilities.supportsImages} /> 支持图片</label
      >
    </div>

    <div class="field-grid three">
      <label>
        <span>上下文长度</span>
        <input
          type="number"
          min="1"
          bind:value={draft.capabilities.contextWindow}
          disabled={busy}
        />
      </label>
      <label>
        <span>最大输出 Token</span>
        <input type="number" min="1" bind:value={draft.maxOutputTokens} disabled={busy} />
      </label>
      <label>
        <span>超时（秒）</span>
        <input type="number" min="1" max="600" bind:value={draft.timeoutSeconds} disabled={busy} />
      </label>
    </div>

    <label>
      <span>自定义 Header</span>
      <textarea
        rows="3"
        bind:value={draft.customHeadersText}
        placeholder="X-App-Name: DeskAide&#10;HTTP-Referer: https://example.com"
        disabled={busy}></textarea>
      <small>每行一个 Name: Value；Authorization、Cookie、X-API-Key 等敏感 Header 禁止保存。</small>
    </label>

    {#if error}<p class="error">{error}</p>{/if}
    {#if notice}<p class="notice">{notice}</p>{/if}

    <div class="actions">
      {#if profile}
        <button
          class="danger"
          type="button"
          onclick={remove}
          disabled={busy || !canDeleteProfile(profile, activeProfileId)}
          title={profile.id === activeProfileId ? '正在使用的模型不能删除' : '删除 Profile'}
          >删除</button
        >
      {/if}
      <span></span>
      <button type="button" onclick={testConnection} disabled={busy || !draft.id}>测试连接</button>
      <button class="primary" type="submit" disabled={busy || validationErrors.length > 0}>
        {busy ? '处理中…' : '保存'}
      </button>
    </div>
  </form>
{/if}

<style>
  form,
  .readonly-profile {
    display: grid;
    gap: 12px;
  }
  label {
    display: grid;
    gap: 5px;
    color: var(--theme-muted-strong);
    font-size: 11px;
  }
  label > span {
    color: var(--theme-heading);
    font-weight: 650;
  }
  input,
  textarea {
    width: 100%;
    padding: 8px 9px;
    border: 1px solid var(--theme-border);
    border-radius: 8px;
    outline: none;
    color: var(--theme-text-strong);
    background: var(--theme-input-bg);
    font-size: 11px;
  }
  input:focus,
  textarea:focus {
    border-color: var(--theme-accent);
  }
  textarea {
    min-height: 62px;
    resize: vertical;
  }
  small {
    color: var(--theme-muted);
    font-size: 9px;
    line-height: 1.4;
  }
  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .field-grid.three {
    grid-template-columns: repeat(3, 1fr);
  }
  .checks {
    display: flex;
    gap: 18px;
  }
  .checks label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .checks input {
    width: auto;
  }
  .actions {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 8px;
    align-items: center;
  }
  button {
    padding: 7px 11px;
    border: 1px solid var(--theme-border);
    border-radius: 8px;
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
    cursor: pointer;
    font-size: 10px;
  }
  button.primary {
    border: 0;
    color: var(--theme-primary-text);
    background: var(--theme-primary-background);
    font-weight: 750;
  }
  button.danger {
    color: var(--theme-danger);
  }
  button:disabled {
    cursor: default;
    opacity: 0.45;
  }
  .error,
  .notice {
    margin: 0;
    padding: 8px;
    border-radius: 7px;
    font-size: 10px;
  }
  .error {
    color: var(--theme-error);
    background: var(--theme-error-background);
  }
  .notice {
    color: var(--theme-success);
    background: var(--theme-success-background);
  }
  .readonly-profile p {
    margin: 0;
    color: var(--theme-muted-strong);
    font-size: 11px;
  }
  h3 {
    margin: 0;
  }
  dl {
    display: grid;
    gap: 6px;
    margin: 0;
  }
  dl div {
    display: flex;
    justify-content: space-between;
  }
  dt {
    color: var(--theme-muted);
    font-size: 10px;
  }
  dd {
    margin: 0;
    color: var(--theme-heading);
    font-size: 10px;
  }
</style>
