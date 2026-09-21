<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { ToolApproval } from './events';
  let {
    approval,
    conversationId,
    turnId,
  }: { approval: ToolApproval; conversationId: string; turnId: string } = $props();
  let persist = $state(false);
  let busy = $state(false);
  let settled = $state(false);
  let error = $state('');
  const argumentsText = $derived.by(() => {
    try {
      return JSON.stringify(JSON.parse(approval.call.arguments), null, 2);
    } catch {
      return approval.call.arguments;
    }
  });
  async function decide(allow: boolean) {
    if (busy || settled) return;
    busy = true;
    error = '';
    try {
      await invoke('approve_tool', {
        conversationId,
        turnId,
        approvalId: approval.approvalId,
        allow,
        persist: allow && persist,
      });
      settled = true;
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<section class="approval" aria-label="工具调用批准">
  <strong>{approval.definition.displayName ?? approval.call.name}</strong>
  <p>
    来源：{approval.definition.source.type === 'mcp'
      ? approval.definition.source.serverName
      : 'DeskAide'} · 需要本次批准
  </p>
  <p>
    权限类别：{(
      {
        readOnly: '只读',
        userData: '用户数据',
        mutating: '修改数据',
        externalSideEffect: '外部副作用',
        unknown: '外部工具，风险未验证',
      } as Record<string, string>
    )[approval.definition.risk] ?? '未验证'}。仅批准这一次调用。
  </p>
  <p>{approval.definition.description}</p>
  <pre>{argumentsText.slice(0, 1200)}{argumentsText.length > 1200 ? '…' : ''}</pre>
  {#if argumentsText.length > 1200}<details>
      <summary>展开全部参数</summary>
      <pre>{argumentsText}</pre>
    </details>{/if}
  {#if approval.sensitiveContext}
    <label
      ><input
        type="checkbox"
        bind:checked={persist}
        disabled={busy || settled}
      />将本次工具参数和结果保存到对话历史</label
    >
    <small
      >本轮含临时桌面上下文。默认仅在本轮使用；勾选后，可能包含敏感信息的参数和结果将保存在本机历史。</small
    >
  {:else}<small>本次工具参数和结果会保存在本机对话历史。</small>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <div>
    <button type="button" disabled={busy || settled} onclick={() => decide(true)}
      >Allow once · 允许本次</button
    ><button type="button" disabled={busy || settled} onclick={() => decide(false)}
      >Deny · 拒绝</button
    >
  </div>
  {#if settled}<small>已提交决定，等待继续…</small>{/if}
</section>

<style>
  .approval {
    padding: 14px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 12px;
    margin: 10px 0;
    color: var(--theme-text);
    background: var(--theme-settings-background);
    overflow-wrap: anywhere;
  }
  p,
  small {
    font-size: 12px;
    line-height: 1.6;
  }
  small {
    display: block;
  }
  pre {
    font-size: 12px;
    white-space: pre-wrap;
    max-height: 240px;
    overflow: auto;
  }
  div {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    flex-wrap: wrap;
  }
  button {
    padding: 7px 10px;
  }
  label {
    font-size: 12px;
    display: flex;
    gap: 6px;
    align-items: start;
  }
</style>
