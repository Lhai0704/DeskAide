import { describe, expect, it, vi } from 'vitest';
import { isTheme, loadTheme, saveTheme, THEME_STORAGE_KEY } from './theme';

function storageWith(value: string | null) {
  return {
    getItem: vi.fn(() => value),
    setItem: vi.fn(),
  };
}

describe('theme preferences', () => {
  it('accepts only supported themes', () => {
    expect(isTheme('dark')).toBe(true);
    expect(isTheme('light')).toBe(true);
    expect(isTheme('system')).toBe(false);
  });

  it('loads a stored theme and otherwise defaults to dark', () => {
    expect(loadTheme(storageWith('light'))).toBe('light');
    expect(loadTheme(storageWith('unknown'))).toBe('dark');
    expect(loadTheme(storageWith(null))).toBe('dark');
  });

  it('persists a selected theme', () => {
    const storage = storageWith(null);
    const root: { dataset: { theme?: string }; style: { colorScheme: string } } = {
      dataset: {},
      style: { colorScheme: '' },
    };

    saveTheme('light', storage, root);

    expect(storage.setItem).toHaveBeenCalledWith(THEME_STORAGE_KEY, 'light');
    expect(root.dataset.theme).toBe('light');
    expect(root.style.colorScheme).toBe('light');
  });
});
