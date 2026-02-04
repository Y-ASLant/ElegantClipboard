import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";
import {
  Settings16Regular,
  Folder16Regular,
  Open16Regular,
  Dismiss16Regular,
  Subtract16Regular,
  ClipboardMultiple16Regular,
  Database16Regular,
  Keyboard16Regular,
  Info16Regular,
  TextDescription16Regular,
} from "@fluentui/react-icons";

interface AppSettings {
  data_path: string;
  max_history_count: number;
  max_content_size_kb: number;
  auto_start: boolean;
  shortcut: string;
  winv_replacement: boolean;
}

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
  const [winvLoading, setWinvLoading] = useState(false);
  const [winvError, setWinvError] = useState("");
  const [winvConfirmDialogOpen, setWinvConfirmDialogOpen] = useState(false);
  const [winvPendingAction, setWinvPendingAction] = useState<"enable" | "disable" | null>(null);
  
  // Shortcut editing state
  const [shortcutDialogOpen, setShortcutDialogOpen] = useState(false);
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [tempShortcut, setTempShortcut] = useState("");
  const [shortcutError, setShortcutError] = useState("");

  // Handle keyboard event for shortcut recording
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!recordingShortcut) return;
    
    e.preventDefault();
    e.stopPropagation();
    
    const parts: string[] = [];
    
    // Modifiers
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");
    
    // Key
    let key = "";
    if (e.code.startsWith("Key")) {
      key = e.code.replace("Key", "");
    } else if (e.code.startsWith("Digit")) {
      key = e.code.replace("Digit", "");
    } else if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) {
      key = e.code; // F1-F12
    } else {
      const keyMap: Record<string, string> = {
        Space: "Space",
        Tab: "Tab",
        Enter: "Enter",
        Backspace: "Backspace",
        Delete: "Delete",
        Escape: "Esc",
        Home: "Home",
        End: "End",
        PageUp: "PageUp",
        PageDown: "PageDown",
        ArrowUp: "Up",
        ArrowDown: "Down",
        ArrowLeft: "Left",
        ArrowRight: "Right",
        Backquote: "`",
      };
      key = keyMap[e.code] || "";
    }
    
    if (key && parts.length > 0) {
      parts.push(key);
      setTempShortcut(parts.join("+"));
      setShortcutError("");
    } else if (!key && parts.length > 0) {
      // Only modifiers pressed, show hint
      setTempShortcut(parts.join("+") + "+...");
    } else if (key && parts.length === 0) {
      setShortcutError("请至少使用一个修饰键 (Ctrl/Alt/Shift/Win)");
    }
  }, [recordingShortcut]);

  // Start/stop recording
  useEffect(() => {
    if (recordingShortcut) {
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [recordingShortcut, handleKeyDown]);

  const startRecording = () => {
    setRecordingShortcut(true);
    setTempShortcut("");
    setShortcutError("");
  };

  const cancelRecording = () => {
    setRecordingShortcut(false);
    setTempShortcut("");
    setShortcutError("");
    setShortcutDialogOpen(false);
  };

  const saveShortcut = async () => {
    if (!tempShortcut || tempShortcut.includes("...")) {
      setShortcutError("请输入完整的快捷键");
      return;
    }
    
    try {
      await invoke("update_shortcut", { newShortcut: tempShortcut });
      await invoke("set_setting", { key: "global_shortcut", value: tempShortcut });
      setSettings({ ...settings, shortcut: tempShortcut });
      setShortcutDialogOpen(false);
      setRecordingShortcut(false);
      setTempShortcut("");
    } catch (error) {
      setShortcutError(`保存失败: ${error}`);
    }
  };

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
      const dataPath = await invoke<string>("get_setting", { key: "data_path" });
      const defaultPath = await invoke<string>("get_default_data_path");
      const maxHistoryCount = await invoke<string>("get_setting", { key: "max_history_count" });
      const maxContentSize = await invoke<string>("get_setting", { key: "max_content_size_kb" });
      const autoStart = await invoke<boolean>("is_autostart_enabled");
      const winvReplacement = await invoke<boolean>("is_winv_replacement_enabled");
      const currentShortcut = await invoke<string>("get_current_shortcut");
      
      setSettings({
        data_path: dataPath || defaultPath || "",
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
      await invoke("set_setting", { key: "data_path", value: settings.data_path });
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

  const selectFolder = async () => {
    try {
      const path = await invoke<string | null>("select_folder_for_settings");
      if (path) {
        setSettings({ ...settings, data_path: path });
      }
    } catch (error) {
      console.error("Failed to select folder:", error);
    }
  };

  const openDataFolder = async () => {
    try {
      await invoke("open_data_folder");
    } catch (error) {
      console.error("Failed to open folder:", error);
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

  // Execute Win+V toggle after user confirms
  const executeWinvToggle = async () => {
    if (!winvPendingAction) return;
    
    setWinvConfirmDialogOpen(false);
    setWinvLoading(true);
    setWinvError("");
    
    try {
      if (winvPendingAction === "enable") {
        await invoke("enable_winv_replacement");
        setSettings({ ...settings, winv_replacement: true });
      } else {
        await invoke("disable_winv_replacement");
        setSettings({ ...settings, winv_replacement: false });
      }
    } catch (error) {
      console.error("Failed to toggle Win+V replacement:", error);
      setWinvError(String(error));
    } finally {
      setWinvLoading(false);
      setWinvPendingAction(null);
    }
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
              {/* General Tab */}
              {activeTab === "general" && (
                <div className="space-y-6">
                  {/* Storage Path */}
                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">数据存储</h3>
                      <p className="text-xs text-muted-foreground">配置剪贴板数据的存储位置</p>
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="data-path" className="text-xs">存储路径</Label>
                      <div className="flex gap-2">
                        <Input
                          id="data-path"
                          value={settings.data_path}
                          onChange={(e) => setSettings({ ...settings, data_path: e.target.value })}
                          placeholder="加载中..."
                          readOnly
                          className="flex-1 h-8 text-sm"
                        />
                        <Button variant="outline" size="icon" onClick={selectFolder} className="h-8 w-8">
                          <Folder16Regular className="w-4 h-4" />
                        </Button>
                        <Button variant="outline" size="icon" onClick={openDataFolder} className="h-8 w-8">
                          <Open16Regular className="w-4 h-4" />
                        </Button>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        留空使用默认路径，修改后需重启应用生效
                      </p>
                    </div>
                  </div>

                  <Separator />

                  {/* History Limit */}
                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">历史记录</h3>
                      <p className="text-xs text-muted-foreground">配置历史记录的存储限制</p>
                    </div>
                    
                    <div className="space-y-3">
                      <div className="flex items-center justify-between">
                        <Label className="text-xs">最大历史记录数</Label>
                        <span className="text-xs font-medium tabular-nums">
                          {settings.max_history_count === 0 ? "无限制" : settings.max_history_count.toLocaleString()}
                        </span>
                      </div>
                      <Slider
                        value={[settings.max_history_count]}
                        onValueChange={(value) => setSettings({ ...settings, max_history_count: value[0] })}
                        min={0}
                        max={10000}
                        step={100}
                      />
                      <p className="text-xs text-muted-foreground">
                        设为 0 表示无限制
                      </p>
                    </div>

                    <div className="space-y-3">
                      <div className="flex items-center justify-between">
                        <Label className="text-xs">单条内容最大大小</Label>
                        <span className="text-xs font-medium tabular-nums">
                          {settings.max_content_size_kb >= 1024 
                            ? `${(settings.max_content_size_kb / 1024).toFixed(1)} MB`
                            : `${settings.max_content_size_kb} KB`
                          }
                        </span>
                      </div>
                      <Slider
                        value={[settings.max_content_size_kb]}
                        onValueChange={(value) => setSettings({ ...settings, max_content_size_kb: value[0] })}
                        min={64}
                        max={10240}
                        step={64}
                      />
                      <p className="text-xs text-muted-foreground">
                        超过此大小的内容将被截断保存
                      </p>
                    </div>
                  </div>

                  <Separator />

                  {/* Startup */}
                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">启动</h3>
                      <p className="text-xs text-muted-foreground">配置应用启动行为</p>
                    </div>
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <Label className="text-xs">开机自启动</Label>
                        <p className="text-xs text-muted-foreground">
                          系统启动时自动运行
                        </p>
                      </div>
                      <Switch
                        checked={settings.auto_start}
                        onCheckedChange={(checked) => setSettings({ ...settings, auto_start: checked })}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* Display Tab */}
              {activeTab === "display" && (
                <div className="space-y-6">
                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">内容预览</h3>
                      <p className="text-xs text-muted-foreground">配置剪贴板卡片的内容显示</p>
                    </div>
                    
                    <div className="space-y-3">
                      <div className="flex items-center justify-between">
                        <Label className="text-xs">预览最大行数</Label>
                        <span className="text-xs font-medium tabular-nums">
                          {cardMaxLines} 行
                        </span>
                      </div>
                      <Slider
                        value={[cardMaxLines]}
                        onValueChange={(value) => setCardMaxLines(value[0])}
                        min={1}
                        max={10}
                        step={1}
                      />
                      <p className="text-xs text-muted-foreground">
                        超过此行数的内容将被截断显示，内容不足时按实际高度显示
                      </p>
                    </div>
                  </div>

                  <Separator />

                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">信息显示</h3>
                      <p className="text-xs text-muted-foreground">配置卡片底部显示的信息</p>
                    </div>
                    
                    <div className="space-y-3">
                      <div className="flex items-center justify-between">
                        <div className="space-y-0.5">
                          <Label className="text-xs">显示时间</Label>
                          <p className="text-xs text-muted-foreground">显示复制的具体时间</p>
                        </div>
                        <Switch checked={showTime} onCheckedChange={setShowTime} />
                      </div>
                      
                      <div className="flex items-center justify-between">
                        <div className="space-y-0.5">
                          <Label className="text-xs">显示字符数</Label>
                          <p className="text-xs text-muted-foreground">显示文本内容的字符数</p>
                        </div>
                        <Switch checked={showCharCount} onCheckedChange={setShowCharCount} />
                      </div>
                      
                      <div className="flex items-center justify-between">
                        <div className="space-y-0.5">
                          <Label className="text-xs">显示大小</Label>
                          <p className="text-xs text-muted-foreground">显示内容的字节大小</p>
                        </div>
                        <Switch checked={showByteSize} onCheckedChange={setShowByteSize} />
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Shortcuts Tab */}
              {activeTab === "shortcuts" && (
                <div className="space-y-6">
                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">自定义快捷键</h3>
                      <p className="text-xs text-muted-foreground">自定义呼出剪贴板的快捷键</p>
                    </div>
                    <div className={cn("space-y-2", settings.winv_replacement && "opacity-50")}>
                      <Label className="text-xs">呼出快捷键</Label>
                      <div className="flex gap-2">
                        <Input
                          value={settings.shortcut}
                          readOnly
                          className="flex-1 h-8 text-sm font-mono bg-muted"
                        />
                        <Button 
                          variant="outline" 
                          size="sm" 
                          className="h-8"
                          onClick={() => setShortcutDialogOpen(true)}
                          disabled={settings.winv_replacement}
                        >
                          修改
                        </Button>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {settings.winv_replacement 
                          ? "已启用 Win+V，自定义快捷键已禁用" 
                          : "点击修改按钮自定义快捷键"}
                      </p>
                    </div>
                  </div>

                  <Separator />

                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">使用 Win+V</h3>
                      <p className="text-xs text-muted-foreground">用 Win+V 替代系统剪贴板</p>
                    </div>
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <div className="space-y-0.5">
                          <Label className="text-xs">启用 Win+V</Label>
                          <p className="text-xs text-muted-foreground">
                            替代系统剪贴板（将禁用自定义快捷键）
                          </p>
                        </div>
                        <Switch
                          checked={settings.winv_replacement}
                          disabled={winvLoading}
                          onCheckedChange={(checked) => {
                            // Open confirmation dialog
                            setWinvPendingAction(checked ? "enable" : "disable");
                            setWinvConfirmDialogOpen(true);
                          }}
                        />
                      </div>
                      {winvLoading && (
                        <p className="text-xs text-muted-foreground">正在修改系统设置，请稍候...</p>
                      )}
                      {winvError && (
                        <p className="text-xs text-destructive">{winvError}</p>
                      )}
                      <p className="text-xs text-amber-500">
                        注意：此操作会修改注册表并重启 Windows 资源管理器
                      </p>
                    </div>
                  </div>

                  <Separator />

                  <div className="space-y-4">
                    <div>
                      <h3 className="text-sm font-medium">当前生效</h3>
                      <p className="text-xs text-muted-foreground">
                        {settings.winv_replacement ? "使用 Win+V 呼出剪贴板" : `使用 ${settings.shortcut} 呼出剪贴板`}
                      </p>
                    </div>
                    <div className="flex items-center justify-between py-2 px-3 rounded-md bg-primary/10 border border-primary/20">
                      <span className="text-sm font-medium">呼出/隐藏窗口</span>
                      <kbd className="pointer-events-none inline-flex h-6 select-none items-center gap-1 rounded border bg-background px-2 font-mono text-xs font-medium">
                        {settings.winv_replacement ? "Win+V" : settings.shortcut}
                      </kbd>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      注：自定义快捷键和 Win+V 只能二选一，不能同时生效
                    </p>
                  </div>
                </div>
              )}

              {/* About Tab */}
              {activeTab === "about" && (
                <div className="space-y-6">
                  <div className="flex flex-col items-center text-center space-y-4 py-6">
                    <div className="h-16 w-16 rounded-2xl bg-primary/10 flex items-center justify-center">
                      <ClipboardMultiple16Regular className="w-8 h-8 text-primary" />
                    </div>
                    <div className="space-y-1">
                      <h3 className="font-semibold text-lg">Clipboard Manager</h3>
                      <p className="text-sm text-muted-foreground">版本 0.1.0</p>
                    </div>
                    <p className="text-sm text-muted-foreground max-w-xs">
                      高性能 Windows 剪贴板管理器，支持文本、图片、HTML、RTF、文件路径
                    </p>
                  </div>

                  <Separator />

                  <div className="space-y-4">
                    <h3 className="text-sm font-medium">技术信息</h3>
                    <div className="space-y-2">
                      {[
                        { label: "框架", value: "Tauri 2.0" },
                        { label: "前端", value: "React + shadcn/ui" },
                        { label: "数据库", value: "SQLite + FTS5" },
                        { label: "平台", value: "Windows" },
                      ].map((item, index) => (
                        <div key={index} className="flex items-center justify-between py-1.5">
                          <span className="text-sm text-muted-foreground">{item.label}</span>
                          <span className="text-sm font-medium">{item.value}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>

      {/* Shortcut Edit Dialog */}
      <Dialog open={shortcutDialogOpen} onOpenChange={(open) => {
        if (!open) cancelRecording();
        else setShortcutDialogOpen(open);
      }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>修改快捷键</DialogTitle>
            <DialogDescription>
              按下新的快捷键组合来设置呼出剪贴板的快捷键
            </DialogDescription>
          </DialogHeader>
          
          <div className="space-y-4 py-4">
            <div 
              className={cn(
                "h-16 flex items-center justify-center rounded-md border-2 border-dashed transition-colors",
                recordingShortcut ? "border-primary bg-primary/5" : "border-muted"
              )}
              onClick={startRecording}
            >
              {recordingShortcut ? (
                <span className="text-lg font-mono font-medium">
                  {tempShortcut || "按下快捷键..."}
                </span>
              ) : (
                <span className="text-sm text-muted-foreground">
                  点击此处开始录入快捷键
                </span>
              )}
            </div>
            
            {shortcutError && (
              <p className="text-sm text-destructive">{shortcutError}</p>
            )}
            
            <p className="text-xs text-muted-foreground">
              快捷键必须包含至少一个修饰键 (Ctrl / Alt / Shift / Win) 加一个普通按键
            </p>
          </div>
          
          <DialogFooter className="flex justify-between sm:justify-between">
            <Button 
              variant="ghost" 
              onClick={() => {
                setTempShortcut("Alt+C");
                setRecordingShortcut(false);
                setShortcutError("");
              }}
              className="text-muted-foreground"
            >
              重置为默认
            </Button>
            <div className="flex gap-2">
              <Button variant="outline" onClick={cancelRecording}>
                取消
              </Button>
              <Button 
                onClick={saveShortcut}
                disabled={!tempShortcut || tempShortcut.includes("...")}
              >
                保存
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Win+V Confirmation Dialog */}
      <Dialog 
        open={winvConfirmDialogOpen} 
        onOpenChange={(open) => {
          if (!open) {
            setWinvConfirmDialogOpen(false);
            setWinvPendingAction(null);
          }
        }}
      >
        <DialogContent className="max-w-[400px]" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {winvPendingAction === "enable" ? "启用 Win+V" : "禁用 Win+V"}
            </DialogTitle>
            <DialogDescription>
              {winvPendingAction === "enable" 
                ? "启用 Win+V 需要修改注册表并重启 Windows 资源管理器，桌面会短暂刷新。"
                : "禁用 Win+V 需要恢复注册表并重启 Windows 资源管理器，桌面会短暂刷新。"
              }
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button 
              variant="outline" 
              onClick={() => {
                setWinvConfirmDialogOpen(false);
                setWinvPendingAction(null);
              }}
            >
              取消
            </Button>
            <Button onClick={executeWinvToggle}>
              确定
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
