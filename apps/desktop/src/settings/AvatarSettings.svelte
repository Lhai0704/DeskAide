<script lang="ts">
  import {
    AVATAR_PACKS,
    discoverPacks,
    avatarPackById,
    type AvatarPackId,
  } from '../avatar/catalog';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {
    loadSettings,
    persistSettings,
    preferencesFor,
    type AvatarSettings,
  } from '../avatar/preferences';
  import { loadAvatarManifest } from '../avatar/manifest';
  import { avatarDefaults, type AvatarPackManifest, type AvatarPreferences } from '../avatar/types';

  interface Props {
    avatarPackId: AvatarPackId;
    onavatarchange: (packId: AvatarPackId) => void;
  }

  let { avatarPackId, onavatarchange }: Props = $props();
  let packs = $state([...AVATAR_PACKS]);
  let directory = $state('');
  let error = $state('');
  let runtimeReady = $state(false);
  let manifest = $state<AvatarPackManifest | null>(null);
  let preferences = $state(avatarDefaults());
  let settings: AvatarSettings = { packId: null, preferences: {} };
  let saving = $state(false);
  let generation = 0;
  async function refresh() {
    try {
      const result = await discoverPacks();
      packs = [...AVATAR_PACKS];
      directory = result.directory;
      runtimeReady = result.runtimeReady;
      settings = await loadSettings();
      await select(settings.packId ?? avatarPackId, false);
    } catch (e) {
      error = String(e);
    }
  }
  async function select(id: string, save = true) {
    if (saving) return;
    saving = true;
    const token = ++generation;
    try {
      const pack = avatarPackById(id) ?? AVATAR_PACKS[0];
      const m = await loadAvatarManifest(pack.root);
      if (token !== generation) return;
      manifest = m;
      preferences = preferencesFor(settings, m);
      if (save) {
        settings = { ...settings, packId: pack.id };
        await persistSettings(settings);
      }
      if (save && token === generation) onavatarchange(pack.id);
      error = '';
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  async function change(patch: Partial<AvatarPreferences>) {
    if (!manifest || saving) return;
    saving = true;
    try {
      const next = { ...preferences, ...patch };
      await persistSettings({
        ...settings,
        preferences: { ...settings.preferences, [manifest.id]: next },
      });
      settings.preferences[manifest.id] = next;
      preferences = next;
      error = '';
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  onMount(() => {
    void refresh();
    return () => {
      generation++;
    };
  });
</script>

<section class="avatar-settings" aria-labelledby="avatar-title">
  <div class="heading">
    <p>ASSISTANT AVATAR</p>
    <h3 id="avatar-title">助手形象</h3>
    <span>选择常驻桌面的助手形象。单击打开助手，按住可拖动位置。</span>
  </div>

  <div class="avatar-options" role="radiogroup" aria-label="助手形象">
    {#each packs as pack (pack.id)}
      <button
        type="button"
        class:selected={manifest?.id === pack.id}
        disabled={saving}
        role="radio"
        aria-checked={manifest?.id === pack.id}
        onclick={() => void select(pack.id)}
      >
        <span class="preview">
          <img src={pack.preview ?? `${pack.root}/idle.png`} alt="" />
        </span>
        <span class="option-copy">
          <strong>{pack.name}</strong>
          <small>{pack.description}</small>
        </span>
        <span class="radio" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
  {#if manifest?.renderer === 'live2d' || manifest?.renderer === 'vrm'}
    <div class="controls">
      {#each [['mouseTracking', '注视鼠标'], ['idleAnimation', '待机动画'], ['autoBlink', '自动眨眼'], ['motions', '模型动作']] as [key, label] (key)}
        <label
          ><input
            type="checkbox"
            disabled={saving}
            checked={preferences[key as keyof AvatarPreferences] === true}
            onchange={(e) => void change({ [key]: e.currentTarget.checked })}
          />{label}</label
        >
      {/each}
      <label
        >模型大小 <input
          type="range"
          min="0.5"
          max="2"
          step="0.05"
          value={preferences.scale}
          disabled={saving}
          onchange={(e) => void change({ scale: Number(e.currentTarget.value) })}
        /></label
      >
      <label
        >垂直位置 <input
          type="range"
          min="-0.4"
          max="0.4"
          step="0.02"
          value={preferences.verticalPosition}
          disabled={saving}
          onchange={(e) => void change({ verticalPosition: Number(e.currentTarget.value) })}
        /></label
      >
      <button type="button" disabled={saving} onclick={() => void change(avatarDefaults(manifest!))}
        >恢复默认</button
      >
      <small>
        {manifest?.renderer === 'vrm'
          ? '立即生效。角色身体可以拖动和点击，周围空白会点到桌面。模型大小不会扩大窗口。'
          : '立即生效。透明边缘仍占用鼠标区域；模型大小不会扩大窗口。'}
      </small>
    </div>
  {/if}
  <div class="controls">
    <small
      >{runtimeReady
        ? '本地 Live2D runtime 已就绪。VRM 使用内置运行时。'
        : 'Live2D runtime 未安装。静态形象和 VRM 仍可使用。'}</small
    ><small>{directory}</small>
    <button
      type="button"
      onclick={() => void invoke('open_avatar_directory').catch((e) => (error = String(e)))}
      >打开本地形象目录</button
    >
    <button type="button" disabled={saving} onclick={() => void refresh()}>刷新形象列表</button>
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
</section>

<style>
  .controls {
    display: grid;
    gap: 10px;
  }
  .controls label {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--theme-text);
    font-size: 12px;
  }
  .controls small {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .avatar-settings {
    display: grid;
    max-width: 560px;
    gap: 18px;
  }
  .heading p {
    margin: 0 0 3px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.16em;
  }
  h3 {
    margin: 0 0 7px;
    color: var(--theme-text);
    font-size: 17px;
  }
  .heading > span {
    color: var(--theme-muted-strong);
    font-size: 11px;
    line-height: 1.5;
  }
  .avatar-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  button {
    position: relative;
    display: grid;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--theme-border);
    border-radius: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    color: var(--theme-text);
    background: var(--theme-control-bg);
    cursor: pointer;
    text-align: left;
  }
  button:hover {
    border-color: var(--theme-border-strong);
    background: var(--theme-control-hover);
  }
  button.selected {
    border-color: var(--theme-accent-border);
    box-shadow: 0 0 0 1px var(--theme-accent-soft);
  }
  .preview {
    display: grid;
    height: 132px;
    overflow: hidden;
    grid-column: 1 / -1;
    place-items: center;
    border: 1px solid var(--theme-border);
    border-radius: 9px;
    background:
      radial-gradient(circle at 50% 44%, rgb(126 226 255 / 13%), transparent 48%),
      var(--theme-input-bg);
  }
  .preview img {
    width: 124px;
    height: 124px;
    object-fit: contain;
    filter: drop-shadow(0 8px 9px rgb(7 16 29 / 25%));
  }
  .option-copy {
    display: grid;
    min-width: 0;
    gap: 3px;
  }
  strong {
    font-size: 11px;
  }
  small {
    overflow: hidden;
    color: var(--theme-muted);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .radio {
    width: 15px;
    height: 15px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 50%;
  }
  button.selected .radio {
    border: 4px solid var(--theme-accent);
  }
</style>
