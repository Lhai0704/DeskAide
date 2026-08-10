<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import AssistantView from './assistant/AssistantView.svelte';
  import ContextEditor from './assistant/ContextEditor.svelte';
  import AvatarView from './avatar/AvatarView.svelte';

  const windowLabel = getCurrentWindow().label;
</script>

{#if windowLabel === 'avatar'}
  <AvatarView />
{:else if windowLabel === 'assistant'}
  <AssistantView />
{:else if windowLabel === 'context-editor'}
  <ContextEditor />
{:else}
  <main class="unknown-window">未知窗口：{windowLabel}</main>
{/if}
