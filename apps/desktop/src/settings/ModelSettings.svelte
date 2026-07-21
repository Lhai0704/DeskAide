<script lang="ts">
  import type { ModelProfile } from '../assistant/model';
  import ModelProfileForm from './ModelProfileForm.svelte';

  interface Props {
    profiles: ModelProfile[];
    activeProfileId: string;
    onchanged: () => void;
    onclose: () => void;
  }

  let { profiles, activeProfileId, onchanged, onclose }: Props = $props();
  function initialSelection() {
    return activeProfileId;
  }
  let selectedId = $state(initialSelection());
  let creating = $state(false);
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
  }

  async function deleted() {
    creating = false;
    await onchanged();
    selectedId = activeProfileId;
  }
</script>

<section class="settings" aria-label="模型设置">
  <header>
    <div>
      <p>MODEL PROFILES</p>
      <h2>模型设置</h2>
    </div>
    <button class="close" type="button" aria-label="关闭模型设置" onclick={onclose}>×</button>
  </header>
  <div class="settings-body">
    <nav aria-label="模型配置列表">
      {#each profiles as profile (profile.id)}
        <button
          type="button"
          class:selected={!creating && profile.id === selectedId}
          onclick={() => {
            selectedId = profile.id;
            creating = false;
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
      <button class="add" type="button" onclick={() => (creating = true)}>＋ 新建 Profile</button>
    </nav>
    <div class="form-panel">
      {#key creating ? 'new' : selectedId}
        <ModelProfileForm
          profile={creating ? null : selected}
          {activeProfileId}
          onsaved={saved}
          ondeleted={deleted}
        />
      {/key}
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
    border: 1px solid rgb(151 198 255 / 22%);
    border-radius: 20px;
    color: #eaf2ff;
    background: linear-gradient(145deg, rgb(18 28 43 / 99.5%), rgb(8 14 24 / 99.5%));
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 12px;
  }
  header p {
    margin: 0 0 2px;
    color: #7ee2ff;
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
    color: #aebbd0;
    background: rgb(255 255 255 / 6%);
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
    border-right: 1px solid rgb(255 255 255 / 7%);
  }
  nav button {
    display: flex;
    padding: 9px;
    border: 1px solid transparent;
    border-radius: 9px;
    flex-direction: column;
    align-items: flex-start;
    color: #a9b9cf;
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  nav button.selected {
    border-color: rgb(126 226 255 / 18%);
    color: #e6f9ff;
    background: rgb(126 226 255 / 8%);
  }
  nav button.add {
    margin-top: 4px;
    color: #82dbf8;
    border-color: rgb(126 226 255 / 12%);
  }
  nav span {
    font-size: 11px;
  }
  nav small {
    margin-top: 3px;
    color: #6e8098;
    font-size: 8px;
  }
  .form-panel {
    min-height: 0;
    padding: 2px 6px 4px 2px;
    overflow: auto;
  }
</style>
