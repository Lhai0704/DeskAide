export const THEME_STORAGE_KEY = 'deskaide.theme';

export type Theme = 'dark' | 'light';

type ThemeStorage = Pick<Storage, 'getItem' | 'setItem'>;
type ThemeRoot = {
  dataset: { theme?: string };
  style: { colorScheme: string };
};

export function isTheme(value: unknown): value is Theme {
  return value === 'dark' || value === 'light';
}

export function loadTheme(storage: ThemeStorage | undefined = browserStorage()): Theme {
  try {
    const stored = storage?.getItem(THEME_STORAGE_KEY);
    return isTheme(stored) ? stored : 'dark';
  } catch {
    return 'dark';
  }
}

export function applyTheme(theme: Theme, root: ThemeRoot | undefined = browserRoot()) {
  if (!root) return;
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
}

export function saveTheme(
  theme: Theme,
  storage: ThemeStorage | undefined = browserStorage(),
  root: ThemeRoot | undefined = browserRoot(),
) {
  applyTheme(theme, root);
  try {
    storage?.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Keep the selected theme for this session when persistent storage is unavailable.
  }
}

export function initializeTheme(): Theme {
  const theme = loadTheme();
  applyTheme(theme);
  return theme;
}

function browserStorage(): ThemeStorage | undefined {
  if (typeof window === 'undefined') return undefined;
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

function browserRoot(): ThemeRoot | undefined {
  if (typeof document === 'undefined') return undefined;
  return document.documentElement;
}
