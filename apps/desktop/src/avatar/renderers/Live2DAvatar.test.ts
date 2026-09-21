// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, render, waitFor } from '@testing-library/svelte';
import Live2DAvatar from './Live2DAvatar.svelte';
import { avatarDefaults, type Live2DAvatarPackManifest } from '../types';
import type { RendererInput } from '../live2d/model';
const h = vi.hoisted(() => ({
  reject: false,
  instances: [] as {
    update: ReturnType<typeof vi.fn>;
    dispose: ReturnType<typeof vi.fn>;
    pause: ReturnType<typeof vi.fn>;
  }[],
}));
vi.mock('../live2d/model', () => ({
  Live2DRenderer: class {
    constructor() {
      h.instances.push(this);
    }
    load = async () => {
      if (h.reject) throw Error('broken model');
    };
    update = vi.fn();
    dispose = vi.fn();
    pause = vi.fn();
  },
}));
const pack: Live2DAvatarPackManifest = {
  schemaVersion: 3,
  renderer: 'live2d',
  id: 'test',
  name: 'test',
  version: '1',
  alt: 'test',
  model: 'test.model3.json',
  preview: 'p.png',
  defaultWidth: 240,
  defaultHeight: 320,
};
const input: RendererInput = {
  state: 'idle',
  interaction: 0,
  speakingLevel: 0,
  cursorFocus: null,
  preferences: avatarDefaults(pack),
};
beforeEach(() => {
  h.instances.length = 0;
  h.reject = false;
});
afterEach(cleanup);
it('forwards state and settings changes after asynchronous mount, then disposes', async () => {
  const result = render(Live2DAvatar, { pack, root: '/test', input, onerror: vi.fn() });
  await waitFor(() => expect(h.instances[0].update).toHaveBeenCalled());
  const speaking: RendererInput = {
    ...input,
    state: 'speaking',
    speakingLevel: 0.7,
    preferences: { ...input.preferences, motions: false },
  };
  await result.rerender({ input: speaking });
  await waitFor(() => expect(h.instances[0].update).toHaveBeenLastCalledWith(speaking));
  result.unmount();
  expect(h.instances[0].dispose).toHaveBeenCalled();
});
it('isolates loading failure and cleans the failed adapter', async () => {
  h.reject = true;
  const onerror = vi.fn();
  render(Live2DAvatar, { pack, root: '/test', input, onerror });
  await waitFor(() => expect(onerror).toHaveBeenCalledWith('Error: broken model'));
  expect(h.instances[0].dispose).toHaveBeenCalledWith(false);
});
it('recreates a lost context only once and removes its listeners on unmount', async () => {
  const result = render(Live2DAvatar, { pack, root: '/test', input, onerror: vi.fn() });
  const canvas = result.container.querySelector('canvas')!;
  canvas.dispatchEvent(new Event('webglcontextlost', { cancelable: true }));
  expect(h.instances[0].pause).toHaveBeenCalledWith(true);
  canvas.dispatchEvent(new Event('webglcontextrestored'));
  expect(h.instances).toHaveLength(2);
  canvas.dispatchEvent(new Event('webglcontextrestored'));
  expect(h.instances).toHaveLength(2);
  result.unmount();
  canvas.dispatchEvent(new Event('webglcontextrestored'));
  expect(h.instances).toHaveLength(2);
});
