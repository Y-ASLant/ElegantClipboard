import type { Page } from "@playwright/test";

export interface FixtureItem {
  id: number;
  content_type: "text" | "files" | "image";
  text_content: string | null;
  html_content: null;
  rtf_content: null;
  image_path: string | null;
  file_paths: string | null;
  content_hash: string;
  preview: string | null;
  byte_size: number;
  image_width: null;
  image_height: null;
  is_pinned: boolean;
  is_favorite: boolean;
  favorite_order: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
  access_count: number;
  last_accessed_at: null;
  char_count: null;
  source_app_name: null;
  source_app_icon: null;
  group_id: null;
}

export function fixtureItem(id: number, type: FixtureItem["content_type"], content: string): FixtureItem {
  return {
    id, content_type: type, text_content: type === "text" ? content : null,
    html_content: null, rtf_content: null, image_path: type === "image" ? content : null,
    file_paths: type === "files" ? JSON.stringify([content]) : null,
    content_hash: `fixture-${id}`, preview: type === "text" ? content : null,
    byte_size: type === "files" ? 234392576 : 32, image_width: null, image_height: null,
    is_pinned: false, is_favorite: false, favorite_order: 0, sort_order: id,
    created_at: "2026-10-04T10:00:00", updated_at: "2026-10-04T10:00:00",
    access_count: 0, last_accessed_at: null, char_count: null,
    source_app_name: null, source_app_icon: null, group_id: null,
  };
}

export interface FixtureOptions {
  items: FixtureItem[];
  locale?: "zh-CN" | "en" | "zh-TW";
  operation?: (command: string, args: Record<string, unknown>) => Promise<unknown>;
  missingIds?: number[];
  pasteMoveToTop?: boolean;
}

export async function installTauriFixture(page: Page, options: FixtureOptions) {
  const locale = options.locale ?? "zh-CN";
  const settings: Record<string, string> = {
    language: locale, tray_icon_visible: "true", max_image_size_kb: "51200",
    plugin_webdav_enabled: "false", plugin_translate_enabled: "false",
    ui_settings_json: JSON.stringify({ onboardingCompleted: true, keyboardNavigation: true,
      searchAutoFocus: false, searchAutoClear: false, autoResetState: false,
      textPreviewEnabled: false, imagePreviewEnabled: false, colorTheme: "default",
      windowEffect: "none", showSourceApp: false, showDragAreaIndicator: false,
      copySound: false, pasteSound: false, pasteMoveToTop: options.pasteMoveToTop ?? false }),
  };
  let rows = [...options.items];
  await page.exposeFunction("__fixtureInvoke", async (command: string, args: Record<string, unknown> = {}) => {
    switch (command) {
      case "get_setting": return settings[String(args.key)] ?? null;
      case "get_settings_batch": {
        if (!Array.isArray(args.keys) || !args.keys.every((key: unknown) => typeof key === "string")) throw new Error("Invalid fixture settings keys");
        return Object.fromEntries(args.keys.map((key: string) => [key, settings[key] ?? ""]));
      }
      case "get_all_settings": return settings;
      case "set_setting": settings[String(args.key)] = String(args.value); return null;
      case "get_clipboard_items": {
        const query = typeof args.search === "string" ? args.search.toLowerCase() : "";
        return rows.filter((row) => (!query || (row.preview ?? row.file_paths ?? "").toLowerCase().includes(query)) && (!args.contentType || row.content_type === args.contentType) && (!args.favoriteOnly || row.is_favorite));
      }
      case "get_clipboard_item": return rows.find((row) => row.id === args.id) ?? null;
      case "get_clipboard_count": return rows.length;
      case "get_groups": return [];
      case "is_window_pinned": return false;
      case "get_system_accent_color": return null;
      case "batch_get_item_file_status": {
        if (!Array.isArray(args.ids) || !args.ids.every((id: unknown) => typeof id === "number" && Number.isInteger(id))) throw new Error("Invalid fixture item ids");
        return Object.fromEntries(args.ids.map((id: number) => {
        const row = rows.find((item) => item.id === id);
        if (!row) return [id, null];
        const paths: unknown = JSON.parse(row.file_paths ?? "[]");
        if (!Array.isArray(paths) || !paths.every((path: unknown) => typeof path === "string")) throw new Error("Invalid fixture paths");
        const exists = !options.missingIds?.includes(id);
        return [id, { all_exist: exists, clipboard_usable: exists, resolved_paths: paths, checks: Object.fromEntries(paths.map((path) => [path, { exists, is_dir: false }])), too_large: true }];
        }));
      }
      case "check_files_exist": {
        if (!Array.isArray(args.paths) || !args.paths.every((path: unknown) => typeof path === "string")) throw new Error("Invalid fixture image paths");
        return Object.fromEntries(args.paths.map((path: string) => [path, { exists: true, is_dir: false }]));
      }
      case "set_active_group":
      case "update_tray_language":
      case "set_keyboard_nav_enabled":
      case "set_window_effect":
      case "sync_preview_presentation":
      case "sync_preview_window_effects":
      case "hide_text_preview":
      case "hide_image_preview": return null;
      case "copy_to_clipboard":
      case "paste_content":
      case "paste_content_as_plain":
      case "paste_as_path":
      case "paste_text_direct":
      case "merge_paste_content":
      case "save_file_as":
      case "show_in_explorer":
      case "get_item_file_status":
      case "bump_item_to_top": return options.operation ? options.operation(command, args) : null;
      case "delete_clipboard_item":
        if (options.operation) await options.operation(command, args);
        rows = rows.filter((row) => row.id !== args.id);
        return null;
      default: throw new Error(`Unconfigured fixture command: ${command}`);
    }
  });
  await page.addInitScript(() => {
    interface Transport {
      __fixtureInvoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
      __fixtureEmit?: (event: string, payload: unknown) => void;
      __TAURI_INTERNALS__?: unknown;
      __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown;
    }
    // These in-process fixture bindings are installed by Playwright before app code.
    const fixture = window as unknown as Window & Transport;
    const callbacks = new Map<number, (payload: unknown) => void>();
    const listeners = new Map<number, { event: string; handler: number }>();
    let nextId = 0;
    fixture.__fixtureEmit = (event, payload) => {
      for (const [id, listener] of listeners) if (listener.event === event) callbacks.get(listener.handler)?.({ event, id, payload });
    };
    fixture.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback(callback: (payload: unknown) => void) { const id = ++nextId; callbacks.set(id, callback); return id; },
      unregisterCallback(id: number) { callbacks.delete(id); },
      convertFileSrc(path: string) { return path; },
      async invoke(command: string, args: Record<string, unknown> = {}) {
        if (command === "plugin:event|listen") {
          const id = ++nextId;
          listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
          return id;
        }
        if (command === "plugin:event|unlisten") { listeners.delete(Number(args.eventId)); return null; }
        if (command === "plugin:event|emit" || command === "plugin:event|emit_to") {
          fixture.__fixtureEmit?.(String(args.event), args.payload);
          return null;
        }
        if (command === "plugin:window|is_visible") return true;
        if (command === "plugin:window|is_focused") return false;
        return fixture.__fixtureInvoke(command, args);
      },
    };
    fixture.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(_event: string, id: number) { listeners.delete(id); } };
  });
}
