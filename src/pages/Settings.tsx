import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";
import {
  Settings16Regular,
  Dismiss16Regular,
  Subtract16Regular,
  Database16Regular,
  Keyboard16Regular,
  Info16Regular,
  TextDescription16Regular,
} from "@fluentui/react-icons";
import { AboutTab } from "@/components/settings/AboutTab";
import { GeneralTab, GeneralSettings } from "@/components/settings/GeneralTab";
import { DisplayTab } from "@/components/settings/DisplayTab";
import { ShortcutsTab, ShortcutSettings } from "@/components/settings/ShortcutsTab";

interface AppSettings extends GeneralSettings, ShortcutSettings {}

type TabType = "general" | "display" | "shortcuts" | "about";

const navItems: { id: TabType; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: "general", label: "常规设置", icon: Database16Regular },
  { id: "display", label: "显示设置", icon: TextDescription16Regular },
  { id: "shortcuts", label: "快捷键", icon: Keyboard16Regular },
  { id: "about", label: "关于", icon: Info16Regular },
];

export function Settings() {
  const [activeTab, setActiveTab] = useState<TabType>("general");
  const { 
    cardMaxLines, setCardMaxLines,
    showTime, setShowTime,
    showCharCount, setShowCharCount,
    showByteSize, setShowByteSize,
  } = useUISettings();
  const [settings, setSettings] = useState<AppSettings>({
    data_path: "",
    max_history_count: 1000,
    max_content_size_kb: 1024,
    auto_start: false,
    shortcut: "Alt+C",
    winv_replacement: false,
  });
  const [, setLoading] = useState(false);

  // Show window after content is loaded (prevent white flash)
  useEffect(() => {
    const settingsWindow = getCurrentWindow();
    requestAnimationFrame(() => {
      settingsWindow.show();
      settingsWindow.setFocus();
    });
  }, []);

  useEffect(() => {
    loadSettings();
  }, []);

  // Auto save when settings change
  useEffect(() => {
    const timer = setTimeout(() => {
      if (settings.data_path !== "" || settings.max_history_count !== 1000 || settings.max_content_size_kb !== 1024) {
        saveSettings();
      }
    }, 500);
    return () => clearTimeout(timer);
  }, [settings.max_history_count, settings.max_content_size_kb, settings.auto_start]);

  const loadSettings = async () => {
    try {
      // Data path is now stored in config.json, not database
      const dataPath = await invoke<string>("get_default_data_path");
      const maxHistoryCount = await invoke<string>("get_setting", { key: "max_history_count" });
      const maxContentSize = await invoke<string>("get_setting", { key: "max_content_size_kb" });
      const autoStart = await invoke<boolean>("is_autostart_enabled");
      const winvReplacement = await invoke<boolean>("is_winv_replacement_enabled");
      const currentShortcut = await invoke<string>("get_current_shortcut");
      
      setSettings({
        data_path: dataPath || "",
        max_history_count: maxHistoryCount ? parseInt(maxHistoryCount) : 1000,
        max_content_size_kb: maxContentSize ? parseInt(maxContentSize) : 1024,
        auto_start: autoStart,
        shortcut: currentShortcut || "Alt+C",
        winv_replacement: winvReplacement,
      });
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  };

  const saveSettings = async () => {
    setLoading(true);
    try {
      // Save settings to database (data_path is handled separately by GeneralTab with migration)
      await invoke("set_setting", { key: "max_history_count", value: settings.max_history_count.toString() });
      await invoke("set_setting", { key: "max_content_size_kb", value: settings.max_content_size_kb.toString() });
      
      if (settings.auto_start) {
        await invoke("enable_autostart");
      } else {
        await invoke("disable_autostart");
      }
      
      console.log("Settings saved");
    } catch (error) {
      console.error("Failed to save settings:", error);
    } finally {
      setLoading(false);
    }
  };

  const minimizeWindow = async () => {
    const window = getCurrentWindow();
    await window.minimize();
  };

  const closeWindow = async () => {
    const window = getCurrentWindow();
    await window.close();
  };

  return (
    <div className="h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3">
      {/* Title Bar Card */}
      <Card className="shrink-0">
        <div
          className="h-11 flex items-center justify-between px-4 select-none"
          data-tauri-drag-region
        >
          <div className="flex items-center gap-3">
            <Settings16Regular className="w-5 h-5 text-muted-foreground" />
            <span className="text-sm font-semibold">设置</span>
          </div>
          <div className="flex" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
            <button
              onClick={minimizeWindow}
              className="w-9 h-9 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-colors"
            >
              <Subtract16Regular className="w-4 h-4" />
            </button>
            <button
              onClick={closeWindow}
              className="w-9 h-9 flex items-center justify-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground rounded-md transition-colors"
            >
              <Dismiss16Regular className="w-4 h-4" />
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
                        "w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors",
                        activeTab === item.id
                          ? "bg-primary text-primary-foreground"
                          : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
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

        {/* Right Content */}
        <div className="flex-1 overflow-auto">
          <Card className="h-full">
            <CardContent className="p-4 h-full overflow-auto">
              {activeTab === "general" && (
                <GeneralTab
                  settings={settings}
                  onSettingsChange={(newSettings) => setSettings({ ...settings, ...newSettings })}
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
                />
              )}

              {activeTab === "shortcuts" && (
                <ShortcutsTab
                  settings={settings}
                  onSettingsChange={(newSettings) => setSettings({ ...settings, ...newSettings })}
                />
              )}

              {activeTab === "about" && (
                <AboutTab />
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
