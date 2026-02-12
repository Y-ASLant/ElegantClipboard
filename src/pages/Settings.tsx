import { useState, useEffect, useRef } from "react";
import {
  Settings16Filled,
  Options16Filled,
  Eye16Filled,
  Keyboard16Filled,
  Info16Filled,
  Database16Filled,
  PaintBrush16Filled,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AboutTab } from "@/components/settings/AboutTab";
import { DataTab, DataSettings } from "@/components/settings/DataTab";
import { DisplayTab } from "@/components/settings/DisplayTab";
import { GeneralTab, GeneralSettings } from "@/components/settings/GeneralTab";
import {
  ShortcutsTab,
  ShortcutSettings,
} from "@/components/settings/ShortcutsTab";
import { ThemeTab } from "@/components/settings/ThemeTab";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";

interface AppSettings extends GeneralSettings, ShortcutSettings, DataSettings {}

type TabType = "general" | "data" | "display" | "theme" | "shortcuts" | "about";

const navItems: {
  id: TabType;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}[] = [
  { id: "general", label: "常规设置", icon: Options16Filled },
  { id: "data", label: "数据管理", icon: Database16Filled },
  { id: "display", label: "显示设置", icon: Eye16Filled },
  { id: "theme", label: "外观主题", icon: PaintBrush16Filled },
  { id: "shortcuts", label: "快捷按键", icon: Keyboard16Filled },
  { id: "about", label: "关于", icon: Info16Filled },
];

export function Settings() {
  const [activeTab, setActiveTab] = useState<TabType>("general");
  const {
    cardMaxLines,
    setCardMaxLines,
    showTime,
    setShowTime,
    showCharCount,
    setShowCharCount,
    showByteSize,
    setShowByteSize,
    showSourceApp,
    setShowSourceApp,
    sourceAppDisplay,
    setSourceAppDisplay,
    imagePreviewEnabled,
    setImagePreviewEnabled,
    previewZoomStep,
    setPreviewZoomStep,
    previewPosition,
    setPreviewPosition,
    imageAutoHeight,
    setImageAutoHeight,
    imageMaxHeight,
    setImageMaxHeight,
  } = useUISettings();
  const [settings, setSettings] = useState<AppSettings>({
    data_path: "",
    max_history_count: 1000,
    max_content_size_kb: 1024,
    auto_start: false,
    admin_launch: false,
    is_running_as_admin: false,
    follow_cursor: true,
    shortcut: "Alt+C",
    winv_replacement: false,
  });
  const settingsLoadedRef = useRef(false);
  const [themeReady, setThemeReady] = useState(false);

  // 主题加载完成后显示窗口（此时过渡被禁用，主题色瞬间就位）
  // 启用过渡后再加载设置，开关会有完整的状态切换动画
  useEffect(() => {
    initTheme().then(async () => {
      const win = getCurrentWindow();
      document.body.getBoundingClientRect();
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      win.show();
      win.setFocus();
      await new Promise((r) => requestAnimationFrame(r));
      setThemeReady(true);
      loadSettings();
    });
  }, []);

  // ESC to close settings window
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const hasOverlay = document.querySelector(
          '[role="dialog"], [data-radix-popper-content-wrapper]'
        );
        if (!hasOverlay) {
          getCurrentWindow().close();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Auto save when settings change (skip until initial load completes)
  useEffect(() => {
    if (!settingsLoadedRef.current) return;
    const timer = setTimeout(() => {
      saveSettings();
    }, 500);
    return () => clearTimeout(timer);
  }, [
    settings.max_history_count,
    settings.max_content_size_kb,
    settings.auto_start,
    settings.admin_launch,
    settings.follow_cursor,
  ]);

  const loadSettings = async () => {
    try {
      const [
        dataPath,
        maxHistoryCount,
        maxContentSize,
        followCursor,
        autoStart,
        adminLaunch,
        isRunningAsAdmin,
        winvReplacement,
        currentShortcut,
      ] = await Promise.all([
        invoke<string>("get_default_data_path"),
        invoke<string>("get_setting", { key: "max_history_count" }),
        invoke<string>("get_setting", { key: "max_content_size_kb" }),
        invoke<string>("get_setting", { key: "follow_cursor" }),
        invoke<boolean>("is_autostart_enabled"),
        invoke<boolean>("is_admin_launch_enabled"),
        invoke<boolean>("is_running_as_admin"),
        invoke<boolean>("is_winv_replacement_enabled"),
        invoke<string>("get_current_shortcut"),
      ]);

      setSettings({
        data_path: dataPath || "",
        max_history_count: maxHistoryCount ? parseInt(maxHistoryCount) : 1000,
        max_content_size_kb: maxContentSize ? parseInt(maxContentSize) : 1024,
        auto_start: autoStart,
        admin_launch: adminLaunch,
        is_running_as_admin: isRunningAsAdmin,
        follow_cursor: followCursor !== "false",
        shortcut: currentShortcut || "Alt+C",
        winv_replacement: winvReplacement,
      });
      settingsLoadedRef.current = true;
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  };

  const saveSettings = async () => {
    try {
      // Save settings to database (data_path is handled separately by GeneralTab with migration)
      await invoke("set_setting", {
        key: "max_history_count",
        value: settings.max_history_count.toString(),
      });
      await invoke("set_setting", {
        key: "max_content_size_kb",
        value: settings.max_content_size_kb.toString(),
      });
      await invoke("set_setting", {
        key: "follow_cursor",
        value: settings.follow_cursor.toString(),
      });

      if (settings.auto_start) {
        await invoke("enable_autostart");
      } else {
        await invoke("disable_autostart");
      }

      // Handle admin launch setting
      if (settings.admin_launch) {
        await invoke("enable_admin_launch");
      } else {
        await invoke("disable_admin_launch");
      }
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  };

  return (
    <div className={cn("h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3", !themeReady && "[&_*]:!transition-none")}>
      {/* Title Bar Card */}
      <Card className="shrink-0">
        <div
          className="h-11 flex items-center justify-between px-4 select-none"
          data-tauri-drag-region
        >
          <div className="flex items-center gap-3">
            <Settings16Filled className="w-5 h-5 text-muted-foreground" />
            <span className="text-sm font-semibold">设置</span>
          </div>
          <div
            className="flex gap-1"
            style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
          >
            <button
              onClick={() => getCurrentWindow().minimize()}
              className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-colors"
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <rect
                  x="2"
                  y="5.5"
                  width="8"
                  height="1"
                  rx="0.5"
                  fill="currentColor"
                />
              </svg>
            </button>
            <button
              onClick={() => getCurrentWindow().close()}
              className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground rounded-md transition-colors"
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  d="M2.5 2.5L9.5 9.5M9.5 2.5L2.5 9.5"
                  stroke="currentColor"
                  strokeWidth="1.2"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </div>
        </div>
      </Card>

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden gap-3">
        {/* Left Navigation */}
        <div className="w-44 shrink-0">
          <Card className="h-full">
            <CardContent className="p-2">
              <nav className="space-y-1">
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
            </CardContent>
          </Card>
        </div>

        {/* Right Content - Full width with scrollbar at edge */}
        {activeTab === "about" ? (
          <div key="about" className="flex-1 flex flex-col gap-3 animate-settings-in">
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

              {activeTab === "display" && (
                <DisplayTab
                  cardMaxLines={cardMaxLines}
                  setCardMaxLines={setCardMaxLines}
                  showTime={showTime}
                  setShowTime={setShowTime}
                  showCharCount={showCharCount}
                  setShowCharCount={setShowCharCount}
                  showByteSize={showByteSize}
                  setShowByteSize={setShowByteSize}
                  showSourceApp={showSourceApp}
                  setShowSourceApp={setShowSourceApp}
                  sourceAppDisplay={sourceAppDisplay}
                  setSourceAppDisplay={setSourceAppDisplay}
                  imagePreviewEnabled={imagePreviewEnabled}
                  setImagePreviewEnabled={setImagePreviewEnabled}
                  previewZoomStep={previewZoomStep}
                  setPreviewZoomStep={setPreviewZoomStep}
                  previewPosition={previewPosition}
                  setPreviewPosition={setPreviewPosition}
                  imageAutoHeight={imageAutoHeight}
                  setImageAutoHeight={setImageAutoHeight}
                  imageMaxHeight={imageMaxHeight}
                  setImageMaxHeight={setImageMaxHeight}
                />
              )}

              {activeTab === "theme" && <ThemeTab />}

              {activeTab === "shortcuts" && (
                <ShortcutsTab
                  settings={settings}
                  onSettingsChange={(newSettings) =>
                    setSettings({ ...settings, ...newSettings })
                  }
                />
              )}
            </div>
          </ScrollArea>
        )}
      </div>
    </div>
  );
}
