<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { avatarAssetUrl, loadAvatarManifest } from './manifest';
  import type { AvatarPackManifest, AvatarStateName } from './types';

  const MENU_WIDTH = 112;
  const MENU_HEIGHT = 44;

  let manifest: AvatarPackManifest | null = null;
  let state: AvatarStateName = 'idle';
  let error = '';
  let lastPosition: { x: number; y: number } | null = null;
  let menuOpen = false;
  let menuX = 0;
  let menuY = 0;

  onMount(async () => {
    const avatarWindow = getCurrentWindow();
    lastPosition = await avatarWindow.outerPosition();
    try {
      manifest = await loadAvatarManifest();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : '助手形象资源包加载失败';
    }
  });

  async function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    menuOpen = false;
    event.preventDefault();
    const avatarWindow = getCurrentWindow();
    const start = lastPosition ?? (await avatarWindow.outerPosition());
    state = 'activated';

    try {
      await avatarWindow.startDragging();
      const end = await avatarWindow.outerPosition();
      lastPosition = end;
      const distance = Math.hypot(end.x - start.x, end.y - start.y);
      if (distance < 5) await invoke('toggle_assistant');
    } finally {
      window.setTimeout(() => (state = 'idle'), 180);
    }
  }

  function onContextMenu(event: MouseEvent) {
    event.preventDefault();
    const maxX = Math.max(0, window.innerWidth - MENU_WIDTH);
    const maxY = Math.max(0, window.innerHeight - MENU_HEIGHT);
    menuX = Math.min(event.clientX, maxX);
    menuY = Math.min(event.clientY, maxY);
    menuOpen = true;
  }

  function closeMenu() {
    menuOpen = false;
  }

  async function exitApp(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    menuOpen = false;
    await invoke('exit_app');
  }
</script>

<svelte:window
  oncontextmenu={onContextMenu}
  onclick={closeMenu}
  onkeydown={(event) => {
    if (event.key === 'Escape') closeMenu();
  }}
/>

<button class="avatar" type="button" aria-label="打开 DeskAide" onpointerdown={onPointerDown}>
  {#if manifest}
    <img src={avatarAssetUrl(manifest, state)} alt={manifest.states[state].alt} draggable="false" />
  {:else if error}
    <span class="fallback" title={error}>DA</span>
  {:else}
    <span class="loading" aria-label="正在加载助手形象"></span>
  {/if}
</button>

{#if menuOpen}
  <div class="menu" style:left="{menuX}px" style:top="{menuY}px">
    <button class="menu-item" type="button" onclick={exitApp}>退出</button>
  </div>
{/if}

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

  .menu {
    position: fixed;
    z-index: 20;
    min-width: 104px;
    padding: 4px;
    border: 1px solid rgb(255 255 255 / 14%);
    border-radius: 10px;
    background: rgb(18 28 42 / 96%);
    box-shadow: 0 10px 24px rgb(0 0 0 / 35%);
    backdrop-filter: blur(10px);
  }

  .menu-item {
    display: block;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    border-radius: 7px;
    color: #eaf6ff;
    background: transparent;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  .menu-item:hover,
  .menu-item:focus-visible {
    outline: 0;
    background: rgb(126 226 255 / 16%);
  }

  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
</style>
