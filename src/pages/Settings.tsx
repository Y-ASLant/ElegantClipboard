import { useState, useEffect, useRef } from "react";
import {
  Settings16Regular,
  Options16Regular,
  Database16Regular,
  LayoutColumnTwo16Regular,
  Color16Regular,
  Keyboard16Regular,
  Info16Regular,
  Filter16Regular,
  ArrowSync16Regular,
  Translate16Regular,
  ScanText16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AboutTab } from "@/components/settings/AboutTab";
import { SyncTab } from "@/components/settings/SyncTab";
import { AppFilterTab } from "@/components/settings/AppFilterTab";
import { DataTab, DataSettings } from "@/components/settings/DataTab";
import { DisplayTab } from "@/components/settings/DisplayTab";
import { GeneralTab, GeneralSettings } from "@/components/settings/GeneralTab";
import {
  ShortcutsTab,
  ShortcutSettings,
} from "@/components/settings/ShortcutsTab";
import { ThemeTab } from "@/components/settings/ThemeTab";
import { TranslateTab } from "@/components/settings/TranslateTab";
import { OcrTab } from "@/components/settings/OcrTab";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";

interface AppSettings extends GeneralSettings, ShortcutSettings, DataSettings {}

const VALID_POSITION_MODES = new Set(["follow_cursor", "screen_center", "fixed_position"]);
function normalizePositionMode(raw: string | null | undefined): import("@/components/settings/GeneralTab").PositionMode {
  if (raw && VALID_POSITION_MODES.has(raw)) return raw as import("@/components/settings/GeneralTab").PositionMode;
  return "follow_cursor";
}

type TabType = "general" | "display" | "theme" | "data" | "appfilter" | "audio" | "shortcuts" | "ocr" | "translate" | "sync" | "about";

const navItems: {
  id: TabType;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}[] = [
  { id: "general", label: "常规设置", icon: Options16Regular },
  { id: "display", label: "显示设置", icon: LayoutColumnTwo16Regular },
  { id: "theme", label: "外观主题", icon: Color16Regular },
  { id: "data", label: "数据管理", icon: Database16Regular },
  { id: "appfilter", label: "监听过滤", icon: Filter16Regular },
  { id: "shortcuts", label: "快捷按键", icon: Keyboard16Regular },
  { id: "ocr", label: "OCR 识别", icon: ScanText16Regular },
  { id: "translate", label: "条目翻译", icon: Translate16Regular },
  { id: "sync", label: "云端同步", icon: ArrowSync16Regular },
  { id: "about", label: "关于软件", icon: Info16Regular },
];

export function Settings() {
  const [activeTab, setActiveTab] = useState<TabType>("general");
  
  const [settings, setSettings] = useState<AppSettings>({
    data_path: "",
    max_history_count: 1000,
    max_content_size_kb: 1024,
    max_image_size_kb: 0,
    max_file_size_kb: 0,
    max_video_size_kb: 0,
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
  const [themeReady, setThemeReady] = useState(false);
  const [appVersion, setAppVersion] = useState("0.0.0");

  useEffect(() => {
    invoke<string>("get_app_version").then(setAppVersion).catch(console.error);
  }, []);

  // 主题加载完成后显示窗口（此时过渡被禁用，主题色瞬间就位）
  // 启用过渡后再加载设置，开关会有完整的状态切换动画
  useEffect(() => {
    initTheme().then(async () => {
      const win = getCurrentWindow();
      document.body.getBoundingClientRect();
      await new Promise((r) =>
        requestAnimationFrame(() => requestAnimationFrame(r)),
      );
      await new Promise((r) => setTimeout(r, 30));
      win.show();
      win.setFocus();
      await new Promise((r) => requestAnimationFrame(r));
      setThemeReady(true);
      loadSettings();
    });
  }, []);

  // ESC 关闭设置窗口
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const hasOverlay = document.querySelector(
          '[role="dialog"], [data-radix-popper-content-wrapper]',
        );
        if (!hasOverlay) {
          getCurrentWindow().close();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // 设置变更时自动保存（跳过初始加载）
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
    settings.max_file_size_kb,
    settings.max_video_size_kb,
    settings.auto_cleanup_days,
    settings.auto_start,
    settings.admin_launch,
  ]);

  const loadSettings = async () => {
    try {
      const [
        dataPath,
        maxHistoryCount,
        maxContentSize,
        maxImageSize,
        maxFileSize,
        maxVideoSize,
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
        invoke<string>("get_setting", { key: "max_file_size_kb" }),
        invoke<string>("get_setting", { key: "max_video_size_kb" }),
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
        max_history_count: maxHistoryCount ? parseInt(maxHistoryCount) : 1000,
        max_content_size_kb: maxContentSize ? parseInt(maxContentSize) : 1024,
        max_image_size_kb: maxImageSize ? parseInt(maxImageSize) : 0,
        max_file_size_kb: maxFileSize ? parseInt(maxFileSize) : 0,
        max_video_size_kb: maxVideoSize ? parseInt(maxVideoSize) : 0,
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
      settingsLoadedRef.current = true;
    } catch (error) {
      logError("Failed to load settings:", error);
    }
  };

  const saveSettings = async () => {
    try {
      // 保存设置到数据库（data_path 由 GeneralTab 单独处理迁移）
      await invoke("set_setting", {
        key: "max_history_count",
        value: settings.max_history_count.toString(),
      });
      await invoke("set_setting", {
        key: "max_content_size_kb",
        value: settings.max_content_size_kb.toString(),
      });
      await invoke("set_setting", {
        key: "max_image_size_kb",
        value: settings.max_image_size_kb.toString(),
      });
      await invoke("set_setting", {
        key: "max_file_size_kb",
        value: settings.max_file_size_kb.toString(),
      });
      await invoke("set_setting", {
        key: "max_video_size_kb",
        value: settings.max_video_size_kb.toString(),
      });
      await invoke("set_setting", {
        key: "auto_cleanup_days",
        value: settings.auto_cleanup_days.toString(),
      });
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
    }
  };

  return (
    <div
      className={cn(
        "h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3",
        !themeReady && "**:transition-none!",
      )}
    >
      <WindowTitleBar
        icon={<Settings16Regular className="w-5 h-5 text-muted-foreground" />}
        title="设置"
      />

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden gap-3">
        {/* Left Navigation */}
        <div className="w-44 shrink-0">
          <Card className="h-full">
            <CardContent className="p-2 h-full flex flex-col">
              <nav className="space-y-1 flex-1">
                {navItems.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      onClick={() => setActiveTab(item.id)}
                      className={cn(
                        "w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors duration-200",
                        activeTab === item.id
                          ? "bg-primary text-primary-foreground shadow-sm"
                          : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                      )}
                    >
                      <Icon className="w-4 h-4" />
                      {item.label}
                    </button>
                  );
                })}
              </nav>
              <div className="pt-2 mt-2 border-t px-2 space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-[11px] text-muted-foreground">
                    版本号
                  </span>
                  <span className="text-[11px] font-medium text-primary">
                    v{appVersion}
                  </span>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Right Content - Full width with scrollbar at edge */}
        {activeTab === "about" ? (
          <div
            key="about"
            className="flex-1 flex flex-col gap-3 animate-settings-in"
          >
            <AboutTab />
          </div>
        ) : (
          <ScrollArea className="flex-1">
            <div key={activeTab} className="space-y-3 animate-settings-in">
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

              {activeTab === "shortcuts" && (
                <ShortcutsTab
                  settings={settings}
                  onSettingsChange={(newSettings) =>
                    setSettings({ ...settings, ...newSettings })
                  }
                />
              )}

              {activeTab === "ocr" && <OcrTab />}

              {activeTab === "translate" && <TranslateTab />}

              {activeTab === "sync" && <SyncTab />}
            </div>
          </ScrollArea>
        )}
      </div>
    </div>
  );
}

