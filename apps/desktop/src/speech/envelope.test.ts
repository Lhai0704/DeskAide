import { it, expect } from 'vitest';
import { MouthEnvelope } from './envelope';
it('uses RMS with attack/release and closes on silence/reset', () => {
  const e = new MouthEnvelope();
  const loud = new Float32Array(1024).fill(0.2);
  const first = e.update(loud, 0.033);
  expect(first).toBeGreaterThan(0);
  expect(first).toBeLessThan(1);
  const peak = e.update(loud, 0.1);
  expect(peak).toBeGreaterThan(first);
  const release = e.update(new Float32Array(1024), 0.033);
  expect(release).toBeGreaterThan(0);
  expect(release).toBeLessThan(peak);
  for (let i = 0; i < 30; i++) e.update(new Float32Array(1024), 0.033);
  expect(e.level).toBe(0);
  e.update(loud, 0.033);
  e.reset();
  expect(e.level).toBe(0);
});
