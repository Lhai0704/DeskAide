// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
import ToolApprovalCard from './ToolApprovalCard.svelte';
import type { ToolApproval } from './events';
const approval: ToolApproval = {
  approvalId: 'a',
  call: { id: 'call', name: 'echo', arguments: '{"text":"secret"}' },
  definition: {
    id: 'tool',
    name: 'echo',
    description: 'Echo data',
    parameters: {},
    source: { type: 'mcp', serverId: 's', serverName: 'Local fixture' },
    risk: 'unknown',
    revision: 1,
  },
  sensitiveContext: true,
};
afterEach(() => {
  cleanup();
  mock.invoke.mockReset();
});
describe('tool approval consent', () => {
  it('shows source and parameters, defaults retention off, submits only once', async () => {
    mock.invoke.mockResolvedValue(undefined);
    render(ToolApprovalCard, { approval, conversationId: 'c', turnId: 't' });
    expect(screen.getByText(/Local fixture/)).toBeTruthy();
    expect(screen.getByText(/"secret"/)).toBeTruthy();
    expect((screen.getByRole('checkbox') as HTMLInputElement).checked).toBe(false);
    const button = screen.getByRole('button', { name: /Allow once/ });
    await fireEvent.click(button);
    await fireEvent.click(button);
    await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
    expect(mock.invoke).toHaveBeenCalledWith('approve_tool', {
      conversationId: 'c',
      turnId: 't',
      approvalId: 'a',
      allow: true,
      persist: false,
    });
  });
  it('sends separate retention consent only for Allow once', async () => {
    mock.invoke.mockResolvedValue(undefined);
    render(ToolApprovalCard, { approval, conversationId: 'c', turnId: 't' });
    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: /Allow once/ }));
    expect(mock.invoke.mock.calls[0][1].persist).toBe(true);
  });
  it('denial never grants retention and surfaces stale approval errors', async () => {
    mock.invoke.mockRejectedValue('已失效');
    render(ToolApprovalCard, { approval, conversationId: 'c', turnId: 't' });
    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: /Deny/ }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('已失效'));
    expect(mock.invoke.mock.calls[0][1]).toMatchObject({ allow: false, persist: false });
  });
});
