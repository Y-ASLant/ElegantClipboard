import { useState, useEffect, useRef, useCallback, useLayoutEffect, useMemo } from "react";
import {
  Settings16Regular,
  Options16Regular,
  Database16Regular,
  LayoutColumnTwo16Regular,
  Color16Regular,
  Keyboard16Regular,
  Info16Regular,
  ArrowSync16Regular,
  Speaker216Regular,
  Filter16Regular,
  PlugConnected16Regular,
  Translate16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AboutTab } from "@/components/settings/AboutTab";
import { AppFilterTab } from "@/components/settings/AppFilterTab";
import { AudioTab } from "@/components/settings/AudioTab";
import { DataTab, DataSettings } from "@/components/settings/DataTab";
import { DisplayTab } from "@/components/settings/DisplayTab";
import { GeneralTab, GeneralSettings } from "@/components/settings/GeneralTab";
import { PluginsTab } from "@/components/settings/PluginsTab";
import {
  ShortcutsTab,
  ShortcutSettings,
} from "@/components/settings/ShortcutsTab";
import { SyncTab } from "@/components/settings/SyncTab";
import { ThemeTab } from "@/components/settings/ThemeTab";
import { TranslateTab } from "@/components/settings/TranslateTab";
import { UpdateDialog } from "@/components/settings/UpdateDialog";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { notifyTranslateAvailabilityChanged } from "@/lib/translate-availability";
import { cn } from "@/lib/utils";
import { notifyWebDAVAvailabilityChanged } from "@/lib/webdav-availability";
import { useTranslateSettings } from "@/stores/translate-settings";

interface AppSettings extends GeneralSettings, ShortcutSettings, DataSettings {}

const VALID_POSITION_MODES = new Set(["follow_cursor", "screen_center", "fixed_position"]);
function normalizePositionMode(raw: string | null | undefined): import("@/components/settings/GeneralTab").PositionMode {
  if (raw && VALID_POSITION_MODES.has(raw)) return raw as import("@/components/settings/GeneralTab").PositionMode;
  return "follow_cursor";
}

type TabType = "general" | "display" | "theme" | "data" | "appfilter" | "audio" | "shortcuts" | "plugins" | "webdav" | "translate" | "about";

type NavItem = {
  id: TabType;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  child?: boolean;
};

const BASE_NAV_ITEMS: NavItem[] = [
  { id: "general", labelKey: "settings.nav.general", icon: Options16Regular },
  { id: "display", labelKey: "settings.nav.display", icon: LayoutColumnTwo16Regular },
  { id: "theme", labelKey: "settings.nav.theme", icon: Color16Regular },
  { id: "data", labelKey: "settings.nav.data", icon: Database16Regular },
  { id: "appfilter", labelKey: "settings.nav.appFilter", icon: Filter16Regular },
  { id: "audio", labelKey: "settings.nav.audio", icon: Speaker216Regular },
  { id: "shortcuts", labelKey: "settings.nav.shortcuts", icon: Keyboard16Regular },
  { id: "plugins", labelKey: "settings.nav.plugins", icon: PlugConnected16Regular },
  { id: "about", labelKey: "settings.nav.about", icon: Info16Regular },
];
type NavIndicator = {
  visible: boolean;
  top: number;
  left: number;
  width: number;
  height: number;
};

export function Settings() {
  const { t, locale } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabType>("general");
  const [suppressTransition, setSuppressTransition] = useState(false);
  const [pluginsEnabled, setPluginsEnabled] = useState<Record<string, boolean>>({ webdav: false, translate: false });
  const navRef = useRef<HTMLElement>(null);
  const [navIndicator, setNavIndicator] = useState<NavIndicator>({
    visible: false,
    top: 0,
    left: 0,
    width: 0,
    height: 0,
  });

  const handlePluginToggle = useCallback(async (id: string, value: boolean) => {
    setPluginsEnabled((prev) => ({ ...prev, [id]: value }));
    if (!value && activeTab === id) setActiveTab("plugins");
    try {
      await invoke("set_setting", { key: `plugin_${id}_enabled`, value: value ? "true" : "false" });
      if (value) {
        if (id === "webdav") {
          await invoke("webdav_enable_plugin");
        }
      } else {
        if (id === "webdav") {
          await invoke("set_setting", { key: "webdav_enabled", value: "false" });
        } else if (id === "translate") {
          const store = useTranslateSettings.getState();
          store.setEnabled(false);
          store.setTranslateSelectionEnabled(false);
          try {
            await invoke("update_translate_selection_shortcut", { newShortcut: "" });
          } catch (error) {
            logError("Failed to unregister translate shortcut:", error);
          }
        }
      }
    } catch (e) {
      logError(`保存插件 ${id} 设置失败:`, e);
    }
    if (id === "webdav") {
      notifyWebDAVAvailabilityChanged();
    } else if (id === "translate") {
      notifyTranslateAvailabilityChanged();
    }
  }, [activeTab]);
  const navItems = useMemo(
    () => {
      const findNav = (id: TabType) => BASE_NAV_ITEMS.find((item) => item.id === id)!;
      return [
        ...BASE_NAV_ITEMS.filter((item) => item.id !== "plugins" && item.id !== "about"),
        findNav("plugins"),
        ...(pluginsEnabled.webdav
          ? [{ id: "webdav" as TabType, labelKey: "settings.nav.webdav", icon: ArrowSync16Regular, child: true }]
          : []),
        ...(pluginsEnabled.translate
          ? [{ id: "translate" as TabType, labelKey: "settings.nav.translate", icon: Translate16Regular, child: true }]
          : []),
        findNav("about"),
      ];
    },
    [pluginsEnabled.webdav, pluginsEnabled.translate, locale, t],
  );

  const activeTabRef = useRef(activeTab);
  activeTabRef.current = activeTab;

  const updateNavIndicator = useCallback(() => {
    const nav = navRef.current;
    if (!nav) return;

    const activeEl = nav.querySelector<HTMLElement>(`[data-nav-id="${activeTabRef.current}"]`);
    if (!activeEl) {
      setNavIndicator((prev) =>
        prev.visible ? { ...prev, visible: false } : prev,
      );
      return;
    }

    const next: NavIndicator = {
      visible: true,
      top: activeEl.offsetTop,
      left: activeEl.offsetLeft,
      width: activeEl.offsetWidth,
      height: activeEl.offsetHeight,
    };

    setNavIndicator((prev) =>
      prev.visible === next.visible &&
      prev.top === next.top &&
      prev.left === next.left &&
      prev.width === next.width &&
      prev.height === next.height
        ? prev
        : next,
    );
  }, []);

  // ResizeObserver 仅在挂载时创建一次，通过 ref 读取最新 activeTab
  useEffect(() => {
    const nav = navRef.current;
    if (!nav) return;

    const observer = new ResizeObserver(updateNavIndicator);
    observer.observe(nav);

    return () => {
      observer.disconnect();
    };
  }, [updateNavIndicator]);

  // 切换 Tab 或插件增减时同步更新指示器位置
  useLayoutEffect(() => {
    updateNavIndicator();
  }, [updateNavIndicator, activeTab, pluginsEnabled.webdav, pluginsEnabled.translate]);

  // tab 切换后一帧解除 transition 抑制，避免 Switch 等组件入场动画
  useEffect(() => {
    if (!suppressTransition) return;
    const id = requestAnimationFrame(() => setSuppressTransition(false));
    return () => cancelAnimationFrame(id);
  }, [suppressTransition, activeTab]);
  
  const [settings, setSettings] = useState<AppSettings>({
    data_path: "",
    max_history_count: 10000,
    max_content_size_kb: 1024,
    max_image_size_kb: 51200,
    auto_cleanup_days: 30,
    auto_start: false,
    admin_launch: false,
    is_running_as_admin: false,
    is_portable: false,
    position_mode: "follow_cursor",
    shortcut: "Alt+C",
    winv_replacement: false,
    log_to_file: false,
    log_file_path: "",
  });
  const settingsLoadedRef = useRef(false);
  const [appVersion, setAppVersion] = useState("0.0.0");
  const [buildTime, setBuildTime] = useState("—");
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);

  useEffect(() => {
    void invoke<boolean>("take_pending_update_dialog").then((pending) => {
      if (pending) setUpdateDialogOpen(true);
    });
    const unlistenOpen = listen("open-update-dialog", () => setUpdateDialogOpen(true));
    const unlistenAuto = listen("auto-update-available", () => setUpdateDialogOpen(true));
    return () => {
      void unlistenOpen.then((fn) => fn());
      void unlistenAuto.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    invoke<string>("get_app_version").then(setAppVersion).catch(console.error);
    invoke<string>("get_build_time").then(setBuildTime).catch(console.error);
  }, []);

  // 加载插件启用状态
  useEffect(() => {
    invoke<Record<string, string>>("get_settings_batch", { keys: ["plugin_webdav_enabled", "plugin_translate_enabled"] })
      .then((m) => setPluginsEnabled({ webdav: m["plugin_webdav_enabled"] === "true", translate: m["plugin_translate_enabled"] === "true" }))
      .catch((e) => logError("加载插件设置失败:", e));
  }, []);

  // ESC 关闭设置窗口
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const hasOverlay = document.querySelector(
          '[role="dialog"], [data-radix-popper-content-wrapper]',
        );
        if (!hasOverlay && !document.body.hasAttribute("data-translate-recording")) {
          getCurrentWindow().close();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // 设置变更时自动保存（跳过初始加载）
  // loadSettings 完成后延迟设置 loaded 标志，确保 setSettings 触发的 effect 被跳过
  useEffect(() => {
    if (!settingsLoadedRef.current) return;
    const timer = setTimeout(() => {
      saveSettings();
    }, 500);
    return () => clearTimeout(timer);
  }, [
    settings.max_history_count,
    settings.max_content_size_kb,
    settings.max_image_size_kb,
    settings.auto_cleanup_days,
    settings.auto_start,
    settings.admin_launch,
  ]);

  const loadSettings = useCallback(async () => {
    try {
      const [
        dataPath,
        maxHistoryCount,
        maxContentSize,
        maxImageSize,
        autoCleanupDays,
        positionMode,
        autoStart,
        adminLaunch,
        isRunningAsAdmin,
        isPortable,
        winvReplacement,
        currentShortcut,
        logToFile,
        logFilePath,
      ] = await Promise.all([
        invoke<string>("get_default_data_path"),
        invoke<string>("get_setting", { key: "max_history_count" }),
        invoke<string>("get_setting", { key: "max_content_size_kb" }),
        invoke<string>("get_setting", { key: "max_image_size_kb" }),
        invoke<string>("get_setting", { key: "auto_cleanup_days" }),
        invoke<string | null>("get_setting", { key: "position_mode" }),
        invoke<boolean>("is_autostart_enabled"),
        invoke<boolean>("is_admin_launch_enabled"),
        invoke<boolean>("is_running_as_admin"),
        invoke<boolean>("is_portable_mode"),
        invoke<boolean>("is_winv_replacement_enabled"),
        invoke<string>("get_current_shortcut"),
        invoke<boolean>("is_log_to_file_enabled"),
        invoke<string>("get_log_file_path"),
      ]);

      setSettings({
        data_path: dataPath || "",
        max_history_count: maxHistoryCount ? parseInt(maxHistoryCount) : 10000,
        max_content_size_kb: maxContentSize ? parseInt(maxContentSize) : 1024,
        max_image_size_kb: maxImageSize ? parseInt(maxImageSize) : 51200,
        auto_cleanup_days: autoCleanupDays ? parseInt(autoCleanupDays) : 30,
        auto_start: autoStart,
        admin_launch: adminLaunch,
        is_running_as_admin: isRunningAsAdmin,
        is_portable: isPortable,
        position_mode: normalizePositionMode(positionMode),
        shortcut: currentShortcut || "Alt+C",
        winv_replacement: winvReplacement,
        log_to_file: logToFile,
        log_file_path: logFilePath || "",
      });
      requestAnimationFrame(() => {
        settingsLoadedRef.current = true;
      });
    } catch (error) {
      logError("Failed to load settings:", error);
      requestAnimationFrame(() => {
        settingsLoadedRef.current = true;
      });
    }
  }, []);

  // settings-main 已在 render 前 initTheme；确认 Tauri 窗口已可操作后再解除后端创建守卫。
  useLayoutEffect(() => {
    if ("__TAURI_INTERNALS__" in window) {
      const win = getCurrentWindow();
      void (async () => {
        try {
          await win.show();
          await win.setFocus();
          await invoke("managed_window_ready");
        } catch (error) {
          logError("Failed to initialize settings window:", error);
        }
      })();
    }
    void loadSettings();
  }, [loadSettings]);

  const saveSettings = async () => {
    try {
      // 保存设置到数据库（data_path 由 GeneralTab 单独处理迁移）
      await Promise.all([
        invoke("set_setting", {
          key: "max_history_count",
          value: settings.max_history_count.toString(),
        }),
        invoke("set_setting", {
          key: "max_content_size_kb",
          value: settings.max_content_size_kb.toString(),
        }),
        invoke("set_setting", {
          key: "max_image_size_kb",
          value: settings.max_image_size_kb.toString(),
        }),
        invoke("set_setting", {
          key: "auto_cleanup_days",
          value: settings.auto_cleanup_days.toString(),
        }),
      ]);
      if (settings.auto_start) {
        await invoke("enable_autostart");
      } else {
        await invoke("disable_autostart");
      }

      // 处理管理员启动设置
      if (settings.admin_launch) {
        await invoke("enable_admin_launch");
      } else {
        await invoke("disable_admin_launch");
      }
    } catch (error) {
      logError("Failed to save settings:", error);
      alert(t("common.settingsSaveFailed", { error: String(error) }));
    }
  };

  return (
    <div className="h-screen flex flex-col bg-page-shell overflow-hidden p-3 gap-3">
      <WindowTitleBar
        icon={<Settings16Regular className="w-5 h-5 text-muted-foreground" />}
        title={t("settings.title")}
      />

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden gap-3">
        {/* Left Navigation */}
        <div className="w-44 shrink-0 min-h-0">
          <Card className="h-full overflow-hidden">
            <CardContent className="p-2 h-full min-h-0 flex flex-col">
              <nav
                ref={navRef}
                className="relative space-y-0.5 flex-1 min-h-0 overflow-y-auto pr-1"
              >
                <div
                  aria-hidden
                  className={cn(
                    "settings-nav-indicator absolute rounded-md bg-primary pointer-events-none",
                    navIndicator.visible ? "opacity-100" : "opacity-0",
                  )}
                  style={{
                    transform: `translate3d(${navIndicator.left}px, ${navIndicator.top}px, 0)`,
                    width: navIndicator.width,
                    height: navIndicator.height,
                  }}
                />
                {navItems.map((item) => {
                  const Icon = item.icon;
                  const isActive = activeTab === item.id;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      data-nav-id={item.id}
                      onClick={() => { setSuppressTransition(true); setActiveTab(item.id); }}
                      className={cn(
                        "interactive-surface relative z-10 flex items-center rounded-md transition-[color,transform,background-color] duration-200 active:scale-[0.98]",
                        item.child
                          ? "ml-5 w-[calc(100%-1.25rem)] gap-2 px-2.5 py-1.5 text-xs before:absolute before:-left-2.5 before:top-1/2 before:-translate-y-1/2 before:h-3 before:w-px before:rounded-full before:bg-border before:content-['']"
                          : "w-full gap-3 px-3 py-2 text-sm",
                        isActive
                          ? "text-primary-foreground font-medium"
                          : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
                      )}
                    >
                      <Icon
                        className={cn(
                          "shrink-0 transition-transform duration-200",
                          item.child ? "w-3.5 h-3.5" : "w-4 h-4",
                          isActive && "scale-110",
                        )}
                      />
                      <span className="truncate">{t(item.labelKey)}</span>
                    </button>
                  );
                })}
              </nav>
              <div className="shrink-0 pt-2 mt-2 border-t px-2 space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-caption text-muted-foreground">{t("settings.version")}</span>
                  <button
                    type="button"
                    onClick={() => setUpdateDialogOpen(true)}
                    className="text-caption text-foreground transition-surface hover:text-primary hover:underline"
                  >
                    v{appVersion}
                  </button>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-caption text-muted-foreground">{t("settings.buildTime")}</span>
                  <span className="text-caption text-foreground">{buildTime}</span>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Right Content */}
        <div className={cn("flex-1 min-h-0 min-w-0", suppressTransition && "*:transition-none!")}>
          {activeTab === "about" ? (
            <div
              key="about"
              className="flex-1 flex flex-col gap-3"
            >
              <AboutTab />
            </div>
          ) : (
            <ScrollArea key={activeTab} className="h-full">
            <div className="flex flex-col gap-3">
              {activeTab === "general" && (
                <GeneralTab
                  settings={settings}
                  onSettingsChange={(newSettings) =>
                    setSettings({ ...settings, ...newSettings })
                  }
                />
              )}

              {activeTab === "data" && (
                <DataTab
                  settings={settings}
                  onSettingsChange={(newSettings) =>
                    setSettings({ ...settings, ...newSettings })
                  }
                />
              )}

              {activeTab === "appfilter" && <AppFilterTab />}

              {activeTab === "display" && <DisplayTab />}

              {activeTab === "theme" && <ThemeTab />}

              {activeTab === "audio" && <AudioTab />}

              {activeTab === "shortcuts" && (
                <ShortcutsTab
                  settings={settings}
                  onSettingsChange={(newSettings) =>
                    setSettings({ ...settings, ...newSettings })
                  }
                />
              )}

              {activeTab === "plugins" && (
                <PluginsTab
                  enabledMap={pluginsEnabled}
                  onToggle={handlePluginToggle}
                />
              )}

              {activeTab === "webdav" && <SyncTab />}

              {activeTab === "translate" && <TranslateTab />}
            </div>
            </ScrollArea>
          )}
        </div>
      </div>
      <UpdateDialog
        open={updateDialogOpen}
        onOpenChange={setUpdateDialogOpen}
      />
    </div>
  );
}

