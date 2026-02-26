import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";
import { useUISettings } from "@/stores/ui-settings";

export interface GeneralSettings {
  auto_start: boolean;
  admin_launch: boolean;
  is_running_as_admin: boolean;
  follow_cursor: boolean;
  log_to_file: boolean;
  log_file_path: string;
}

interface GeneralTabProps {
  settings: GeneralSettings;
  onSettingsChange: (settings: GeneralSettings) => void;
}

export function GeneralTab({ settings, onSettingsChange }: GeneralTabProps) {
  const autoResetState = useUISettings((s) => s.autoResetState);
  const setAutoResetState = useUISettings((s) => s.setAutoResetState);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const setSearchAutoFocus = useUISettings((s) => s.setSearchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const setSearchAutoClear = useUISettings((s) => s.setSearchAutoClear);
  const keyboardNavigation = useUISettings((s) => s.keyboardNavigation);
  const setKeyboardNavigation = useUISettings((s) => s.setKeyboardNavigation);
  const showCategoryFilter = useUISettings((s) => s.showCategoryFilter);
  const setShowCategoryFilter = useUISettings((s) => s.setShowCategoryFilter);
  const {
    copySound, setCopySound,
    pasteSound, setPasteSound,
    pasteCloseWindow, setPasteCloseWindow,
  } = useUISettings();
  const [adminRestartDialogOpen, setAdminRestartDialogOpen] = useState(false);
  const [pendingAdminLaunch, setPendingAdminLaunch] = useState<boolean | null>(null);
  const [logRestartDialogOpen, setLogRestartDialogOpen] = useState(false);
  const [pendingLogToFile, setPendingLogToFile] = useState<boolean | null>(null);
  const [persistWindowSize, setPersistWindowSize] = useState(true);

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "persist_window_size" })
      .then((v) => setPersistWindowSize(v !== "false"))
      .catch(() => {});
  }, []);

  const togglePersistWindowSize = async (enabled: boolean) => {
    setPersistWindowSize(enabled);
    try {
      await invoke("set_setting", { key: "persist_window_size", value: String(enabled) });
      // 关闭时清除已保存的尺寸
      if (!enabled) {
        await invoke("set_setting", { key: "window_width", value: "" });
        await invoke("set_setting", { key: "window_height", value: "" });
      }
    } catch (error) {
      logError("Failed to save persist_window_size:", error);
    }
  };

  return (
    <>
      <div className="space-y-4">
        {/* Startup Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">启动</h3>
          <p className="text-xs text-muted-foreground mb-4">配置应用启动行为</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">开机自启动</Label>
                <p className="text-xs text-muted-foreground">
                  系统启动时自动运行
                </p>
              </div>
              <Switch
                checked={settings.auto_start}
                onCheckedChange={(checked) => onSettingsChange({ ...settings, auto_start: checked })}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs flex items-center gap-2">
                  以管理员身份启动
                  {settings.is_running_as_admin && (
                    <span className="text-[10px] px-1.5 py-0.5 bg-primary/10 text-primary rounded animate-in fade-in duration-200">
                      当前已提权
                    </span>
                  )}
                </Label>
                <p className="text-xs text-muted-foreground">
                  允许监听任务管理器等高权限窗口的点击
                </p>
              </div>
              <Switch
                checked={settings.admin_launch}
                onCheckedChange={(checked) => {
                  setPendingAdminLaunch(checked);
                  setAdminRestartDialogOpen(true);
                }}
              />
            </div>
          </div>
        </div>

        {/* Window Behavior Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">窗口</h3>
          <p className="text-xs text-muted-foreground mb-4">配置窗口显示行为</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">跟随鼠标</Label>
                <p className="text-xs text-muted-foreground">
                  窗口显示在鼠标位置附近
                </p>
              </div>
              <Switch
                checked={settings.follow_cursor}
                onCheckedChange={(checked) => onSettingsChange({ ...settings, follow_cursor: checked })}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">记住窗口大小</Label>
                <p className="text-xs text-muted-foreground">
                  启用后，手动拖拽调整的窗口大小将被保留
                </p>
              </div>
              <Switch checked={persistWindowSize} onCheckedChange={togglePersistWindowSize} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">自动重置状态</Label>
                <p className="text-xs text-muted-foreground">
                  关闭窗口时重置搜索、分组筛选和滚动位置
                </p>
              </div>
              <Switch
                checked={autoResetState}
                onCheckedChange={setAutoResetState}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">底部分类栏</Label>
                <p className="text-xs text-muted-foreground">
                  显示底部内容类型分类筛选栏
                </p>
              </div>
              <Switch
                checked={showCategoryFilter}
                onCheckedChange={setShowCategoryFilter}
              />
            </div>
          </div>
        </div>

        {/* Search Bar Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">搜索栏</h3>
          <p className="text-xs text-muted-foreground mb-4">配置激活窗口时的搜索栏行为</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">默认聚焦</Label>
                <p className="text-xs text-muted-foreground">
                  激活窗口时，默认聚焦搜索框
                </p>
              </div>
              <Switch
                checked={searchAutoFocus}
                onCheckedChange={setSearchAutoFocus}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">自动清除</Label>
                <p className="text-xs text-muted-foreground">
                  激活窗口时，仅清空搜索框文字
                </p>
              </div>
              <Switch
                checked={searchAutoClear}
                onCheckedChange={setSearchAutoClear}
              />
            </div>
          </div>
        </div>

        {/* Operation Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">操作</h3>
          <p className="text-xs text-muted-foreground mb-4">配置交互与操作行为</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">键盘导航</Label>
                <p className="text-xs text-muted-foreground">
                  方向键选择条目和切换分组、Enter 粘贴、Shift+Enter 纯文本粘贴、Delete 删除
                </p>
              </div>
              <Switch
                checked={keyboardNavigation}
                onCheckedChange={setKeyboardNavigation}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">粘贴后关闭窗口</Label>
                <p className="text-xs text-muted-foreground">
                  非锁定模式下，粘贴后自动关闭窗口
                </p>
              </div>
              <Switch checked={pasteCloseWindow} onCheckedChange={setPasteCloseWindow} />
            </div>
          </div>
        </div>

        {/* Sound Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">提示音</h3>
          <p className="text-xs text-muted-foreground mb-4">操作时播放简短的反馈音效</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">复制提示音</Label>
                <p className="text-xs text-muted-foreground">
                  监听到新内容复制时播放提示音
                </p>
              </div>
              <Switch checked={copySound} onCheckedChange={setCopySound} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">粘贴提示音</Label>
                <p className="text-xs text-muted-foreground">
                  点击卡片粘贴时播放提示音
                </p>
              </div>
              <Switch checked={pasteSound} onCheckedChange={setPasteSound} />
            </div>
          </div>
        </div>

        {/* Log Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">日志</h3>
          <p className="text-xs text-muted-foreground mb-4">调试与故障排查</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">保存日志到文件</Label>
                <p className="text-xs text-muted-foreground">
                  日志文件上限 10MB，超出自动轮转
                </p>
              </div>
              <Switch
                checked={settings.log_to_file}
                onCheckedChange={(checked) => {
                  setPendingLogToFile(checked);
                  setLogRestartDialogOpen(true);
                }}
              />
            </div>
            {settings.log_to_file && settings.log_file_path && (
              <p className="text-xs text-muted-foreground break-all">
                路径：{settings.log_file_path}
              </p>
            )}
          </div>
        </div>
      </div>

      {/* Admin Launch Restart Dialog */}
      <Dialog open={adminRestartDialogOpen} onOpenChange={setAdminRestartDialogOpen}>
        <DialogContent className="max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {pendingAdminLaunch ? "启用管理员模式" : "关闭管理员模式"}
            </DialogTitle>
            <DialogDescription>
              此设置需要重启应用后才能生效
            </DialogDescription>
          </DialogHeader>
          
          <DialogFooter className="gap-2">
            <Button
              variant="outline"
              onClick={() => {
                setAdminRestartDialogOpen(false);
                setPendingAdminLaunch(null);
              }}
            >
              取消
            </Button>
            <Button
              variant="outline"
              onClick={async () => {
                if (pendingAdminLaunch !== null) {
                  try {
                    // Directly save to backend
                    if (pendingAdminLaunch) {
                      await invoke("enable_admin_launch");
                    } else {
                      await invoke("disable_admin_launch");
                    }
                    onSettingsChange({ ...settings, admin_launch: pendingAdminLaunch });
                  } catch (error) {
                    alert(`操作失败: ${error}`);
                  }
                }
                setAdminRestartDialogOpen(false);
                setPendingAdminLaunch(null);
              }}
            >
              稍后重启
            </Button>
            <Button
              onClick={async () => {
                if (pendingAdminLaunch !== null) {
                  try {
                    // Directly save to backend before restart
                    if (pendingAdminLaunch) {
                      await invoke("enable_admin_launch");
                    } else {
                      await invoke("disable_admin_launch");
                    }
                    onSettingsChange({ ...settings, admin_launch: pendingAdminLaunch });
                    await invoke("restart_app");
                  } catch (error) {
                    alert(`操作失败: ${error}`);
                    setAdminRestartDialogOpen(false);
                    setPendingAdminLaunch(null);
                  }
                }
              }}
            >
              立即重启
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Log Restart Dialog */}
      <Dialog open={logRestartDialogOpen} onOpenChange={setLogRestartDialogOpen}>
        <DialogContent className="max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {pendingLogToFile ? "启用日志保存" : "关闭日志保存"}
            </DialogTitle>
            <DialogDescription>
              此设置需要重启应用后才能生效
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2">
            <Button
              variant="outline"
              onClick={() => {
                setLogRestartDialogOpen(false);
                setPendingLogToFile(null);
              }}
            >
              取消
            </Button>
            <Button
              variant="outline"
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                  } catch (error) {
                    alert(`操作失败: ${error}`);
                  }
                }
                setLogRestartDialogOpen(false);
                setPendingLogToFile(null);
              }}
            >
              稍后重启
            </Button>
            <Button
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                    await invoke("restart_app");
                  } catch (error) {
                    alert(`操作失败: ${error}`);
                    setLogRestartDialogOpen(false);
                    setPendingLogToFile(null);
                  }
                }
              }}
            >
              立即重启
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
