<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import {
    AVATAR_PACK_CHANGED_EVENT,
    avatarPackById,
    isAvatarPackId,
    loadAvatarPackId,
    type AvatarPackChangedPayload,
    type AvatarPackId,
  } from './catalog';
  import { avatarAssetUrl, loadAvatarManifest } from './manifest';
  import type { AvatarPackManifest, AvatarStateName } from './types';

  let manifest: AvatarPackManifest | null = null;
  let packRoot = avatarPackById(loadAvatarPackId()).root;
  let state: AvatarStateName = 'idle';
  let error = '';
  let lastPosition: { x: number; y: number } | null = null;
  let loadGeneration = 0;

  onMount(() => {
    const avatarWindow = getCurrentWindow();
    void avatarWindow.outerPosition().then((position) => (lastPosition = position));
    void loadAvatarPack(loadAvatarPackId());
    const unlistenAvatarChange = listen<AvatarPackChangedPayload>(
      AVATAR_PACK_CHANGED_EVENT,
      ({ payload }) => {
        if (isAvatarPackId(payload.packId)) void loadAvatarPack(payload.packId);
      },
    );

    return () => {
      void unlistenAvatarChange.then((unlisten) => unlisten());
    };
  });

  async function loadAvatarPack(packId: AvatarPackId) {
    const generation = ++loadGeneration;
    const nextRoot = avatarPackById(packId).root;
    error = '';
    try {
      const nextManifest = await loadAvatarManifest(nextRoot);
      if (generation !== loadGeneration) return;
      packRoot = nextRoot;
      manifest = nextManifest;
    } catch (cause) {
      if (generation !== loadGeneration) return;
      manifest = null;
      error = cause instanceof Error ? cause.message : '助手形象资源包加载失败';
    }
  }

  async function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const avatarWindow = getCurrentWindow();
    const start = lastPosition ?? (await avatarWindow.outerPosition());
    state = 'activated';

    // Prevent assistant blur-hide while this avatar click/drag is in progress.
    await invoke('set_avatar_interacting', { interacting: true });
    try {
      await avatarWindow.startDragging();
      const end = await avatarWindow.outerPosition();
      lastPosition = end;
      const distance = Math.hypot(end.x - start.x, end.y - start.y);
      if (distance < 5) await invoke('toggle_assistant');
    } finally {
      await invoke('set_avatar_interacting', { interacting: false });
      window.setTimeout(() => (state = 'idle'), 180);
    }
  }
</script>

<svelte:window oncontextmenu={(event) => event.preventDefault()} />

<button class="avatar" type="button" aria-label="打开 DeskAide" onpointerdown={onPointerDown}>
  {#if manifest}
    <img
      src={avatarAssetUrl(manifest, state, packRoot)}
      alt={manifest.states[state].alt}
      draggable="false"
    />
  {:else if error}
    <span class="fallback" title={error}>DA</span>
  {:else}
    <span class="loading" aria-label="正在加载助手形象"></span>
  {/if}
</button>

<style>
  .avatar {
    width: 100%;
    height: 100%;
    padding: 3px;
    border: 0;
    outline: 0;
    cursor: grab;
    user-select: none;
    background: transparent;
    -webkit-user-select: none;
  }

  .avatar:active {
    cursor: grabbing;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    filter: drop-shadow(0 8px 9px rgb(7 16 29 / 30%));
    transition: scale 120ms ease;
  }

  .avatar:hover img {
    scale: 1.025;
  }

  .fallback,
  .loading {
    display: grid;
    width: 116px;
    height: 116px;
    margin: auto;
    place-items: center;
    border: 3px solid rgb(126 226 255 / 70%);
    border-radius: 50%;
    color: #dff8ff;
    background: #172437;
    box-shadow: 0 8px 18px rgb(0 0 0 / 28%);
    font-size: 28px;
    font-weight: 750;
  }

  .loading::after {
    width: 28px;
    height: 28px;
    content: '';
    border: 3px solid rgb(255 255 255 / 25%);
    border-top-color: #7ee2ff;
    border-radius: 50%;
    animation: spin 800ms linear infinite;
  }

  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
</style>
