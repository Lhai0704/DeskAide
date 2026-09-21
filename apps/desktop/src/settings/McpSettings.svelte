<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  interface Server {
    id: string;
    name: string;
    enabled: boolean;
    command: string;
    args: string[];
    workingDirectory: string | null;
    revision: number;
  }
  interface Status {
    id: string;
    state: string;
    toolCount: number;
    error: string | null;
  }
  const fresh = (): Server => ({
    id: crypto.randomUUID(),
    name: '',
    enabled: false,
    command: '',
    args: [],
    workingDirectory: null,
    revision: 0,
  });
  let servers = $state<Server[]>([]),
    statuses = $state<Status[]>([]),
    draft = $state<Server>(fresh());
  let busy = $state(false),
    error = $state(''),
    notice = $state('');
  async function refresh() {
    try {
      const result = await invoke<{ servers: Server[]; statuses: Status[]; error: string | null }>(
        'get_mcp_settings',
      );
      servers = result.servers;
      statuses = result.statuses;
      if (result.error) error = result.error;
    } catch (cause) {
      error = String(cause);
    }
  }
  onMount(() => {
    void refresh();
    const timer = setInterval(() => {
      if (!busy) void refresh();
    }, 2000);
    return () => clearInterval(timer);
  });
  async function action(work: () => Promise<unknown>, success: string) {
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const result = await work();
      notice = typeof result === 'number' ? `${success}，发现 ${result} 个工具` : success;
      await refresh();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  function edit(server: Server) {
    draft = structuredClone($state.snapshot(server));
    notice = '';
    error = '';
  }
  async function remove(server: Server) {
    await action(
      () => invoke('delete_mcp_server', { serverId: server.id }),
      '已删除并停止该 server',
    );
    if (draft.id === server.id) draft = fresh();
  }
  function stateLabel(state: string) {
    return (
      (
        { stopped: '未启动', starting: '正在连接', ready: '已连接', failed: '连接失败' } as Record<
          string,
          string
        >
      )[state] ?? state
    );
  }
</script>

<section aria-label="MCP 设置">
  <h3>MCP 工具 · 本地 stdio</h3>
  <p>
    仅连接你信任的本地程序。程序以当前用户权限运行；工具批准不是系统沙箱。启用后按需启动，空闲时退出。
  </p>
  <p>不支持批处理、secret env 或远程 MCP。参数保存在普通设置中，请勿填写密钥。</p>
  {#each servers as server (server.id)}
    {@const status = statuses.find((s) => s.id === server.id)}
    <article>
      <strong>{server.name}</strong><small
        >{server.enabled ? '已启用' : '已禁用'} · {stateLabel(status?.state ?? 'stopped')} · {status?.toolCount ??
          0} 个工具</small
      >
      {#if status?.error}<p>{status.error}</p>{/if}
      <div>
        <button type="button" disabled={busy} onclick={() => edit(server)}
          >编辑 {server.name}</button
        ><button
          type="button"
          disabled={busy}
          onclick={() =>
            action(() => invoke('reconnect_mcp_server', { serverId: server.id }), '连接已刷新')}
          >重连</button
        ><button type="button" disabled={busy} onclick={() => remove(server)}
          >删除 {server.name}</button
        >
      </div>
    </article>
  {/each}
  <button
    type="button"
    disabled={busy}
    onclick={() => {
      draft = fresh();
      notice = '';
    }}>添加 server</button
  >
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void action(() => invoke('save_mcp_server', { server: draft }), '已保存');
    }}
  >
    <label
      >名称<input
        aria-label="MCP 名称"
        bind:value={draft.name}
        required
        maxlength="128"
        disabled={busy}
      /></label
    >
    <label
      >程序<input
        aria-label="MCP 程序"
        bind:value={draft.command}
        placeholder="node.exe 或 python.exe 的路径"
        required
        disabled={busy}
      /></label
    >
    <fieldset disabled={busy}>
      <legend>参数（每项单独传入，不做 shell 解析）</legend>
      {#each draft.args as argument, i (i)}<div>
          <input
            aria-label={`参数 ${i + 1}`}
            value={argument}
            oninput={(e) => {
              draft.args[i] = e.currentTarget.value;
            }}
          /><button
            type="button"
            onclick={() => {
              draft.args = draft.args.filter((_, index) => index !== i);
            }}>移除参数 {i + 1}</button
          >
        </div>{/each}
      <button
        type="button"
        onclick={() => {
          draft.args = [...draft.args, ''];
        }}>添加参数</button
      >
    </fieldset>
    <label
      >工作目录（可选绝对路径）<input
        aria-label="MCP 工作目录"
        value={draft.workingDirectory ?? ''}
        oninput={(e) => {
          draft.workingDirectory = e.currentTarget.value || null;
        }}
        disabled={busy}
      /></label
    >
    <label class="toggle"
      ><input
        type="checkbox"
        bind:checked={draft.enabled}
        disabled={busy}
      />启用，允许聊天按需连接</label
    >
    <div>
      <button type="submit" disabled={busy}>保存 MCP 配置</button><button
        type="button"
        disabled={busy}
        onclick={() =>
          action(
            () => invoke<number>('test_mcp_server', { server: draft }),
            '测试成功，临时连接已清理',
          )}>测试连接</button
      >
    </div>
  </form>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
</section>

<style>
  section {
    display: grid;
    gap: 12px;
  }
  h3,
  p {
    margin: 0;
  }
  p,
  small {
    font-size: 12px;
    line-height: 1.6;
  }
  article,
  form {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 10px;
  }
  label {
    display: grid;
    gap: 5px;
    font-size: 12px;
  }
  label.toggle {
    display: flex;
    align-items: center;
  }
  input {
    min-width: 0;
    padding: 7px;
    border-radius: 5px;
  }
  div {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  fieldset {
    min-width: 0;
    border: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  fieldset input {
    flex: 1;
  }
  button {
    padding: 6px 9px;
  }
  small {
    display: block;
  }
</style>
