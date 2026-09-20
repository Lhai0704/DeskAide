<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  let shortcut = $state('Control+Shift+Space');
  let copilotEnabled = $state(true);
  let ready = $state(false);
  let busy = $state(false);
  let message = $state('');
  let startupError = $state('');
  onMount(() => {
    void (async () => {
      try {
        const settings = await invoke<{ shortcut: string; copilotEnabled: boolean }>(
          'get_shortcut_settings',
        );
        shortcut = settings.shortcut;
        copilotEnabled = settings.copilotEnabled;
        startupError = (await invoke<string | null>('get_shortcut_error')) ?? '';
        ready = true;
      } catch (error) {
        message = String(error);
      }
    })();
  });
  async function save() {
    busy = true;
    message = '';
    try {
      await invoke('save_shortcut_settings', {
        settings: { shortcut: shortcut.trim(), copilotEnabled },
      });
      message = '已保存，立即生效';
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }
</script>

<section aria-labelledby="shortcut-title">
  <h3 id="shortcut-title">激活快捷键</h3>
  <p>在其他应用中也能呼出助手；助手已打开时，将输入焦点切回助手。</p>
  <label class="toggle"
    ><input type="checkbox" bind:checked={copilotEnabled} disabled={!ready || busy} />使用 Copilot
    键激活助手</label
  >
  <p>
    开启后，DeskAide 运行期间接管标准 Copilot 键（Win + Shift + F23），阻止该按键打开 Windows
    搜索。关闭后恢复系统行为。
  </p>
  <label for="activation-shortcut">备用组合键</label>
  <input
    id="activation-shortcut"
    type="text"
    bind:value={shortcut}
    disabled={!ready || busy}
    spellcheck="false"
  />
  <p>例如 Control+Shift+Space 或 Alt+Space。至少包含一个修饰键；若已被占用，保存时会提示。</p>
  <button
    type="button"
    disabled={!ready || busy}
    onclick={() => {
      shortcut = 'Control+Shift+Space';
    }}>恢复默认组合键</button
  >
  <button class="primary" type="button" disabled={!ready || busy} onclick={save}
    >{busy ? '保存中…' : '保存快捷键'}</button
  >
  {#if startupError}<p role="alert">{startupError}</p>{/if}
  <p role="status">{message}</p>
</section>

<style>
  section {
    max-width: 520px;
    color: var(--theme-text);
  }
  h3 {
    margin: 0 0 12px;
    font-size: 17px;
  }
  p {
    color: var(--theme-muted-strong);
    font-size: 12px;
    line-height: 1.7;
  }
  label {
    display: block;
    margin-top: 20px;
    font-size: 13px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  input[type='text'] {
    box-sizing: border-box;
    width: 100%;
    margin: 10px 0 0;
    padding: 11px;
    border: 1px solid var(--theme-border);
    border-radius: 8px;
    background: var(--theme-input-bg);
    color: var(--theme-text);
  }
  button {
    padding: 9px 12px;
    margin: 8px 8px 0 0;
    border: 1px solid var(--theme-border);
    border-radius: 8px;
    color: var(--theme-text);
    background: var(--theme-control-bg);
    cursor: pointer;
  }
  .primary {
    border-color: var(--theme-accent-border);
    background: var(--theme-accent-soft);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
