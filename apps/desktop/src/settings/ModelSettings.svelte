<script lang="ts">
  import type { ModelProfile } from '../assistant/model';
  import ModelProfileForm from './ModelProfileForm.svelte';
  import ThemeSettings from './ThemeSettings.svelte';
  import type { Theme } from './theme';

  interface Props {
    profiles: ModelProfile[];
    activeProfileId: string;
    theme: Theme;
    onchanged: () => void;
    onthemechange: (theme: Theme) => void;
    onclose: () => void;
  }

  let { profiles, activeProfileId, theme, onchanged, onthemechange, onclose }: Props = $props();
  function initialSelection() {
    return activeProfileId;
  }
  let selectedId = $state(initialSelection());
  let creating = $state(false);
  let section = $state<'appearance' | 'models'>('appearance');
  let selected = $derived(profiles.find((profile) => profile.id === selectedId) ?? null);

  $effect(() => {
    if (!creating && !profiles.some((profile) => profile.id === selectedId)) {
      selectedId = activeProfileId;
    }
  });

  async function saved(profile: ModelProfile) {
    creating = false;
    await onchanged();
    selectedId = profile.id;
    section = 'models';
  }

  async function deleted() {
    creating = false;
    await onchanged();
    selectedId = activeProfileId;
  }
</script>

<section class="settings" aria-label="设置">
  <header>
    <div>
      <p>SETTINGS</p>
      <h2>设置</h2>
    </div>
    <button class="close" type="button" aria-label="关闭设置" onclick={onclose}>×</button>
  </header>
  <div class="settings-body">
    <nav aria-label="设置导航">
      <button
        type="button"
        class="appearance"
        class:selected={section === 'appearance'}
        onclick={() => {
          section = 'appearance';
          creating = false;
        }}
      >
        <span>外观</span>
        <small>{theme === 'light' ? '浅色模式' : '深色模式'}</small>
      </button>
      <p class="nav-heading">模型配置</p>
      {#each profiles as profile (profile.id)}
        <button
          type="button"
          class:selected={section === 'models' && !creating && profile.id === selectedId}
          onclick={() => {
            selectedId = profile.id;
            creating = false;
            section = 'models';
          }}
        >
          <span>{profile.name}</span>
          <small
            >{profile.providerType === 'mock'
              ? '离线 Mock'
              : profile.hasApiKey
                ? 'API Key 已设置'
                : 'API Key 未设置'}</small
          >
        </button>
      {/each}
      <button
        class="add"
        class:selected={section === 'models' && creating}
        type="button"
        onclick={() => {
          creating = true;
          section = 'models';
        }}>＋ 新建 Profile</button
      >
    </nav>
    <div class="form-panel">
      {#if section === 'appearance'}
        <ThemeSettings {theme} {onthemechange} />
      {:else}
        {#key creating ? 'new' : selectedId}
          <ModelProfileForm
            profile={creating ? null : selected}
            {activeProfileId}
            onsaved={saved}
            ondeleted={deleted}
          />
        {/key}
      {/if}
    </div>
  </div>
</section>

<style>
  .settings {
    position: absolute;
    z-index: 8;
    inset: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    padding: 18px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 20px;
    color: var(--theme-text);
    background: var(--theme-settings-background);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 12px;
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
  .close {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 8px;
    color: var(--theme-muted-strong);
    background: var(--theme-control-bg);
    font-size: 20px;
    cursor: pointer;
  }
  .settings-body {
    display: grid;
    min-height: 0;
    grid-template-columns: 160px minmax(0, 1fr);
    gap: 14px;
  }
  nav {
    display: flex;
    min-height: 0;
    padding-right: 8px;
    overflow: auto;
    flex-direction: column;
    gap: 6px;
    border-right: 1px solid var(--theme-border);
  }
  nav button {
    display: flex;
    padding: 9px;
    border: 1px solid transparent;
    border-radius: 9px;
    flex-direction: column;
    align-items: flex-start;
    color: var(--theme-muted-strong);
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  nav button.selected {
    border-color: var(--theme-accent-border);
    color: var(--theme-accent-text);
    background: var(--theme-accent-soft);
  }
  nav button.add {
    margin-top: 4px;
    color: var(--theme-accent);
    border-color: var(--theme-accent-border);
  }
  nav span {
    font-size: 11px;
  }
  nav small {
    margin-top: 3px;
    color: var(--theme-muted);
    font-size: 8px;
  }
  .form-panel {
    min-height: 0;
    padding: 2px 6px 4px 2px;
    overflow: auto;
  }
  .nav-heading {
    margin: 9px 8px 1px;
    color: var(--theme-muted);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  nav button.appearance {
    margin-bottom: 2px;
  }
</style>
