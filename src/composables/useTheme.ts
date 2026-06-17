/**
 * Light / dark theme switch.
 *
 * Dark is the default (the platform's dark personality). Setting
 * `data-theme="light"` on <html> flips every `--sf-*` token to the
 * light palette defined in styles/tokens.css. The choice persists to
 * localStorage so it survives reloads.
 */
import { ref } from 'vue';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'solflow.theme';

function readStored(): Theme {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'light' ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

function applyToDom(t: Theme): void {
  const el = document.documentElement;
  if (t === 'light') el.setAttribute('data-theme', 'light');
  else el.removeAttribute('data-theme');
}

// Module-level singleton so every caller shares one reactive value.
const theme = ref<Theme>(readStored());

/** Apply the stored theme to the DOM. Call once at app boot. */
export function initTheme(): void {
  applyToDom(theme.value);
}

export function useTheme() {
  function setTheme(t: Theme): void {
    theme.value = t;
    applyToDom(t);
    try {
      localStorage.setItem(STORAGE_KEY, t);
    } catch {
      // ignore storage failures — choice still applies for this session
    }
  }
  function toggleTheme(): void {
    setTheme(theme.value === 'dark' ? 'light' : 'dark');
  }
  return { theme, setTheme, toggleTheme };
}
