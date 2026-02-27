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

// Subscribers for accent color changes (for ThemeTab preview)
const _accentSubscribers = new Set<(color: string | null) => void>();

function notifyAccentSubscribers() {
  _accentSubscribers.forEach((fn) => fn(_accentColor));
}

function applySharpCorners() {
  const { sharpCorners } = useUISettings.getState();
  document.documentElement.classList.toggle("sharp-corners", sharpCorners);
}

function applyWindowEffect() {
  const { windowEffect } = useUISettings.getState();
  document.documentElement.setAttribute("data-window-effect", windowEffect);
  invoke("set_window_effect", { effect: windowEffect }).catch(() => {
    // Effect not supported on this OS (e.g. Mica/Tabbed on Windows 10) —
    // revert the CSS attribute and the stored setting so the window stays opaque.
    // Use setState directly to avoid re-triggering the setter's invoke call.
    document.documentElement.setAttribute("data-window-effect", "none");
    useUISettings.setState({ windowEffect: "none" });
  });
}

function apply() {
  const { colorTheme } = useUISettings.getState();
  const root = document.documentElement;

  root.classList.remove(...THEME_CLASSES);
  root.style.removeProperty("--system-accent-h");
  root.style.removeProperty("--system-accent-s");
  root.style.removeProperty("--system-accent-l");

  if (colorTheme === "system" && _accentColor) {
    const parts = _accentColor.split(" ");
    root.classList.add("theme-system");
    root.style.setProperty("--system-accent-h", parts[0]);
    root.style.setProperty("--system-accent-s", parts[1] || "65%");
    root.style.setProperty("--system-accent-l", parts[2] || "50%");
  } else if (colorTheme !== "default" && colorTheme !== "system") {
    root.classList.add(`theme-${colorTheme}`);
  }
}

/** Initialize theme system. Safe to call multiple times — only runs once per window. */
export function initTheme(): Promise<void> {
  if (_initialized) return _readyPromise;
  _initialized = true;

  // --- Dark mode ---
  const mq = window.matchMedia("(prefers-color-scheme: dark)");

  function applyDarkMode() {
    const { darkMode } = useUISettings.getState();
    const isDark =
      darkMode === "dark" ? true : darkMode === "light" ? false : mq.matches;
    document.documentElement.classList.toggle("dark", isDark);
  }

  applyDarkMode();
  mq.addEventListener("change", () => applyDarkMode());

  // --- React-free store subscription: re-apply on theme/corners/darkMode change ---
  useUISettings.subscribe((state, prev) => {
    if (state.sharpCorners !== prev.sharpCorners) {
      applySharpCorners();
    }
    if (state.windowEffect !== prev.windowEffect) {
      applyWindowEffect();
    }
    if (state.darkMode !== prev.darkMode) {
      applyDarkMode();
    }
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
    notifyAccentSubscribers();
    apply();
  });

  // --- Initial apply ---
  applySharpCorners();
  applyWindowEffect();
  // Always fetch accent color for ThemeTab preview, regardless of current theme
  invoke<string | null>("get_system_accent_color")
    .then((color) => {
      _accentColor = color;
      notifyAccentSubscribers();
      apply();
    })
    .catch(() => apply())
    .finally(() => _readyResolve?.());

  return _readyPromise;
}

/** Read the cached accent color (for ThemeTab preview). */
export function getAccentColor(): string | null {
  return _accentColor;
}

/** Subscribe to accent color changes. Returns unsubscribe function. */
export function subscribeAccentColor(
  fn: (color: string | null) => void,
): () => void {
  _accentSubscribers.add(fn);
  return () => _accentSubscribers.delete(fn);
}
