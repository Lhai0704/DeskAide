<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import {
    AVATAR_PACK_CHANGED_EVENT,
    avatarPackById,
    discoverPacks,
    loadAvatarPackId,
    type AvatarPackChangedPayload,
  } from './catalog';
  import { loadAvatarManifest, avatarAssetUrl } from './manifest';
  import { loadSettings, persistSettings, preferencesFor } from './preferences';
  import { avatarDefaults, avatarUsesPointerMask, type AvatarPackManifest } from './types';
  import { AvatarBehavior, type Presentation } from './behavior/controller';
  import { AvatarInteraction } from './interaction';
  import { localCursor, type CursorSample } from './live2d/gaze';
  import { noteAvatarPointer, pointHitsAvatar, releaseAvatarPointer } from './live2d/passthrough';
  import StaticAvatar from './renderers/StaticAvatar.svelte';
  import VideoAvatar from './renderers/VideoAvatar.svelte';
  import Live2DAvatar from './renderers/Live2DAvatar.svelte';
  const behavior = new AvatarBehavior(),
    gesture = new AvatarInteraction();
  let manifest = $state<AvatarPackManifest | null>(null),
    root = $state(''),
    error = $state('');
  let preferences = $state(avatarDefaults());
  let presentation = $state(behavior.read(0));
  let cursor = $state<{ x: number; y: number } | null>(null);
  let generation = 0,
    disposed = false;
  let renderKey = $state(0);
  let button: HTMLButtonElement;
  async function load(packId?: string) {
    const token = ++generation;
    error = '';
    try {
      await discoverPacks();
      const settings = await loadSettings();
      const pack =
        avatarPackById(packId ?? settings.packId ?? loadAvatarPackId()) ??
        avatarPackById('default-assistant');
      const next = await loadAvatarManifest(pack.root);
      if (disposed || token !== generation) return;
      behavior.resetInteraction();
      root = pack.root;
      preferences = preferencesFor(settings, next);
      manifest = next;
      renderKey++;
      await invoke('resize_avatar', { width: next.defaultWidth, height: next.defaultHeight });
      if (!settings.packId && !disposed && token === generation)
        await persistSettings({ ...settings, packId: pack.id }).catch((e) => {
          if (!disposed && token === generation) error = `设置保存失败：${String(e)}`;
        });
    } catch (e) {
      if (token === generation && !disposed) {
        error = String(e);
        manifest = null;
      }
    }
  }
  async function refreshSettings() {
    try {
      const settings = await loadSettings();
      if (settings.packId && settings.packId !== manifest?.id) {
        await load();
        return;
      }
      if (manifest) preferences = preferencesFor(settings, manifest);
    } catch (e) {
      error = String(e);
    }
  }
  onMount(() => {
    disposed = false;
    void load();
    let busy = false,
      cursorBusy = false,
      first = true;
    const poll = async () => {
      if (busy) return;
      busy = true;
      try {
        const next = await invoke<Presentation>('get_avatar_presentation');
        if (!disposed) {
          behavior.accept(next, performance.now(), first);
          first = false;
          presentation = behavior.read(performance.now());
        }
      } catch {
        /* Chat is independent of presentation. */
      } finally {
        busy = false;
      }
    };
    void poll();
    const timer = setInterval(() => {
      presentation = behavior.read(performance.now());
      void poll();
    }, 100);
    const cursorTimer = setInterval(() => {
      if (
        cursorBusy ||
        document.hidden ||
        !avatarUsesPointerMask(manifest?.renderer) ||
        !preferences.mouseTracking ||
        error
      )
        return;
      cursorBusy = true;
      void invoke<CursorSample | null>('sample_avatar_cursor')
        .then((s) => {
          if (!disposed) cursor = s ? localCursor(s) : null;
        })
        .catch(() => {
          cursor = null;
        })
        .finally(() => {
          cursorBusy = false;
        });
    }, 34);
    const subscriptions = [
      listen<AvatarPackChangedPayload>(
        AVATAR_PACK_CHANGED_EVENT,
        (e) => void load(e.payload.packId),
      ),
      listen('avatar-settings-changed', () => void refreshSettings()),
      listen('avatar-activated', () => {
        if (!presentation.interaction) activate();
      }),
      getCurrentWindow().onScaleChanged(() => {
        if (manifest)
          void invoke('resize_avatar', {
            width: manifest.defaultWidth,
            height: manifest.defaultHeight,
          });
      }),
    ];
    return () => {
      disposed = true;
      generation++;
      clearInterval(timer);
      clearInterval(cursorTimer);
      for (const p of subscriptions) void p.then((fn) => fn());
      gesture.cancel();
      void invoke('set_avatar_interacting', { interacting: false });
    };
  });
  function activate() {
    behavior.flash('activated', performance.now());
    presentation = behavior.read(performance.now());
  }
  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    if (avatarUsesPointerMask(manifest?.renderer) && !pointHitsAvatar(e.clientX, e.clientY)) {
      releaseAvatarPointer();
      return;
    }
    e.preventDefault();
    gesture.down(e.clientX, e.clientY);
    button.setPointerCapture(e.pointerId);
    void invoke('set_avatar_interacting', { interacting: true });
  }
  async function move(e: PointerEvent) {
    if (avatarUsesPointerMask(manifest?.renderer)) {
      noteAvatarPointer(e.clientX, e.clientY, gesture.active);
    }
    if (!gesture.move(e.clientX, e.clientY)) return;
    try {
      await getCurrentWindow().startDragging();
    } finally {
      gesture.cancel();
      void invoke('set_avatar_interacting', { interacting: false });
    }
  }
  function up(e: PointerEvent) {
    const click = gesture.up();
    if (button.hasPointerCapture(e.pointerId)) button.releasePointerCapture(e.pointerId);
    if (click) {
      activate();
      void invoke('toggle_assistant');
    }
    void invoke('set_avatar_interacting', { interacting: false });
  }
  function cancel() {
    gesture.cancel();
    void invoke('set_avatar_interacting', { interacting: false });
  }
</script>

<svelte:window oncontextmenu={(e) => e.preventDefault()} />
<button
  bind:this={button}
  class="avatar"
  data-state={presentation.state}
  type="button"
  aria-label="打开 DeskAide"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={cancel}
  onpointerleave={() => {
    if (!gesture.active && avatarUsesPointerMask(manifest?.renderer)) releaseAvatarPointer();
  }}
  onlostpointercapture={cancel}
>
  {#if manifest}
    {#if manifest.renderer === 'live2d'}
      {#key renderKey}<Live2DAvatar
          pack={manifest}
          {root}
          input={{ ...presentation, cursorFocus: cursor, preferences }}
          onerror={(message) => (error = message)}
        />{/key}
    {:else if manifest.renderer === 'vrm'}
      {#await import('./renderers/VrmAvatar.svelte') then Vrm}
        {#key renderKey}<Vrm.default
            pack={manifest}
            {root}
            input={{ ...presentation, cursorFocus: cursor, preferences }}
            onerror={(message) => (error = message)}
          />{/key}
      {:catch reason}
        <span class="fallback" title={String(reason)}>DA</span>
      {/await}
    {:else if manifest.renderer === 'video'}
      <VideoAvatar
        src={avatarAssetUrl(
          manifest,
          presentation.state === 'activated' ? 'activated' : 'idle',
          root,
        )}
        alt={manifest.states.idle.alt}
      />
    {:else}<StaticAvatar
        src={avatarAssetUrl(
          manifest,
          presentation.state === 'activated' ? 'activated' : 'idle',
          root,
        )}
        alt={manifest.states.idle.alt}
      />{/if}
  {/if}
  {#if error || !manifest}<span class="fallback" title={error || '正在加载形象'}>DA</span>{/if}
</button>

<style>
  .avatar {
    position: relative;
    width: 100%;
    height: 100%;
    padding: 3px;
    border: 0;
    outline: 0;
    background: transparent;
    cursor: grab;
    user-select: none;
    touch-action: none;
  }
  .avatar:active {
    cursor: grabbing;
  }
  .fallback {
    position: absolute;
    inset: 50% auto auto 50%;
    transform: translate(-50%, -50%);
    display: grid;
    place-items: center;
    width: 100px;
    height: 100px;
    border-radius: 50%;
    color: #dff8ff;
    background: #172437;
    border: 2px solid #7ee2ff;
    font-size: 28px;
  }
</style>
