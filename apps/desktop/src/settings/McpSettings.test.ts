// @vitest-environment jsdom
import { afterEach, it, expect, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
import McpSettings from './McpSettings.svelte';
afterEach(() => {
  cleanup();
  mock.invoke.mockReset();
});
it('adds disabled servers and sends arguments as an array', async () => {
  mock.invoke.mockImplementation(async (command: string) =>
    command === 'get_mcp_settings' ? { servers: [], statuses: [], error: null } : undefined,
  );
  render(McpSettings);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('get_mcp_settings'));
  await fireEvent.input(screen.getByLabelText('MCP 名称'), { target: { value: 'fixture' } });
  await fireEvent.input(screen.getByLabelText('MCP 程序'), { target: { value: 'node.exe' } });
  await fireEvent.click(screen.getByRole('button', { name: '添加参数' }));
  await fireEvent.input(screen.getByLabelText('参数 1'), {
    target: { value: 'path with spaces/server.js' },
  });
  await fireEvent.submit(screen.getByRole('button', { name: '保存 MCP 配置' }).closest('form')!);
  await waitFor(() =>
    expect(mock.invoke.mock.calls.some((c) => c[0] === 'save_mcp_server')).toBe(true),
  );
  const server = mock.invoke.mock.calls.find((c) => c[0] === 'save_mcp_server')![1].server;
  expect(server.enabled).toBe(false);
  expect(server.args).toEqual(['path with spaces/server.js']);
});
it('shows status, tool count, connection failures, and deletion', async () => {
  const server = {
    id: 's',
    name: 'fixture',
    enabled: true,
    command: 'node.exe',
    args: [],
    workingDirectory: null,
    revision: 1,
  };
  mock.invoke.mockImplementation(async (command: string) =>
    command === 'get_mcp_settings'
      ? {
          servers: [server],
          statuses: [{ id: 's', state: 'failed', toolCount: 0, error: '握手失败' }],
          error: null,
        }
      : undefined,
  );
  render(McpSettings);
  await waitFor(() => expect(screen.getByText('握手失败')).toBeTruthy());
  expect(screen.getByText(/0 个工具/)).toBeTruthy();
  await fireEvent.click(screen.getByRole('button', { name: '删除 fixture' }));
  await waitFor(() =>
    expect(mock.invoke).toHaveBeenCalledWith('delete_mcp_server', { serverId: 's' }),
  );
});
