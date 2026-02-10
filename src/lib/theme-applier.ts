/**
 * Module-level theme applier — zero React overhead.
 *
 * Call `initTheme()` once per window. It:
 * - Applies the current color theme class to <html>
 * - Fetches system accent color (one IPC call) and sets --system-accent-h
 * - Listens for WM_SETTINGCHANGE via backend event (color sent in payload, no re-fetch)
 * - Subscribes to zustand store for theme switches
 * - Applies dark mode via matchMedia
 *
 * Returns a Promise that resolves when the theme is fully applied
 * (important for Settings window to delay show() until ready).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useUISettings } from "@/stores/ui-settings";

const THEME_CLASSES = ["theme-emerald", "theme-cyan", "theme-system"];

let _initialized = false;
let _accentColor: string | null = null;
let _readyResolve: (() => void) | null = null;
const _readyPromise = new Promise<void>((resolve) => {
  _readyResolve = resolve;
});

function apply() {
  const { colorTheme } = useUISettings.getState();
  const root = document.documentElement;

  root.classList.remove(...THEME_CLASSES);
  root.style.removeProperty("--system-accent-h");
  root.style.removeProperty("--system-accent-s");

  if (colorTheme === "system" && _accentColor) {
    const parts = _accentColor.split(" ");
    root.classList.add("theme-system");
    root.style.setProperty("--system-accent-h", parts[0]);
    root.style.setProperty("--system-accent-s", parts[1] || "65%");
  } else if (colorTheme !== "default" && colorTheme !== "system") {
    root.classList.add(`theme-${colorTheme}`);
  }
}

/** Initialize theme system. Safe to call multiple times — only runs once per window. */
export function initTheme(): Promise<void> {
  if (_initialized) return _readyPromise;
  _initialized = true;

  // --- Dark mode (sync, immediate) ---
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  document.documentElement.classList.toggle("dark", mq.matches);
  mq.addEventListener("change", (e) => {
    document.documentElement.classList.toggle("dark", e.matches);
  });

  // --- React-free store subscription: re-apply on theme change ---
  useUISettings.subscribe((state, prev) => {
    if (state.colorTheme !== prev.colorTheme) {
      if (state.colorTheme === "system" && !_accentColor) {
        // Switching TO system theme but we don't have the color yet
        invoke<string | null>("get_system_accent_color").then((color) => {
          _accentColor = color;
          apply();
        });
      } else {
        apply();
      }
    }
  });

  // --- Backend pushes new accent color directly (no re-fetch IPC) ---
  listen<string | null>("system-accent-color-changed", (event) => {
    _accentColor = event.payload;
    apply();
  });

  // --- Initial apply ---
  const { colorTheme } = useUISettings.getState();
  if (colorTheme === "system") {
    invoke<string | null>("get_system_accent_color")
      .then((color) => {
        _accentColor = color;
        apply();
      })
      .catch(() => apply())
      .finally(() => _readyResolve?.());
  } else {
    apply();
    _readyResolve?.();
  }

  return _readyPromise;
}

/** Read the cached accent color (for ThemeTab preview). */
export function getAccentColor(): string | null {
  return _accentColor;
}
