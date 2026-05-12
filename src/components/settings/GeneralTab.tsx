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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { LANGUAGE_OPTIONS, useI18n } from "@/i18n";
import { logError } from "@/lib/logger";
import { useUISettings } from "@/stores/ui-settings";

export type PositionMode = "follow_cursor" | "screen_center" | "fixed_position";

export interface GeneralSettings {
  auto_start: boolean;
  admin_launch: boolean;
  is_running_as_admin: boolean;
  is_portable: boolean;
  position_mode: PositionMode;
  log_to_file: boolean;
  log_file_path: string;
}

interface GeneralTabProps {
  settings: GeneralSettings;
  onSettingsChange: (settings: GeneralSettings) => void;
}

export function GeneralTab({ settings, onSettingsChange }: GeneralTabProps) {
  const { locale, setLocale, t } = useI18n();
  const autoResetState = useUISettings((s) => s.autoResetState);
  const setAutoResetState = useUISettings((s) => s.setAutoResetState);
  const windowAnimation = useUISettings((s) => s.windowAnimation);
  const setWindowAnimation = useUISettings((s) => s.setWindowAnimation);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const setSearchAutoFocus = useUISettings((s) => s.setSearchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const setSearchAutoClear = useUISettings((s) => s.setSearchAutoClear);
  const {
    pasteCloseWindow, setPasteCloseWindow,
    pasteMoveToTop, setPasteMoveToTop,
  } = useUISettings();
  const [adminRestartDialogOpen, setAdminRestartDialogOpen] = useState(false);
  const [pendingAdminLaunch, setPendingAdminLaunch] = useState<boolean | null>(null);
  const [logRestartDialogOpen, setLogRestartDialogOpen] = useState(false);
  const [pendingLogToFile, setPendingLogToFile] = useState<boolean | null>(null);
  const [persistWindowSize, setPersistWindowSize] = useState(true);
  const [autoCheckUpdate, setAutoCheckUpdate] = useState(true);


  useEffect(() => {
    invoke<string | null>("get_setting", { key: "persist_window_size" })
      .then((v) => setPersistWindowSize(v !== "false"))
      .catch((error) => {
        logError("Failed to load persist_window_size:", error);
      });
    invoke<string | null>("get_setting", { key: "auto_check_update" })
      .then((v) => setAutoCheckUpdate(v !== "false"))
      .catch((error) => {
        logError("Failed to load auto_check_update:", error);
      });
  }, []);

  const changePositionMode = async (mode: PositionMode) => {
    onSettingsChange({ ...settings, position_mode: mode });
    try {
      await invoke("set_setting", { key: "position_mode", value: mode });
    } catch (error) {
      logError("Failed to save position_mode:", error);
    }
  };

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

  const toggleAutoCheckUpdate = async (enabled: boolean) => {
    setAutoCheckUpdate(enabled);
    try {
      await invoke("set_setting", { key: "auto_check_update", value: String(enabled) });
    } catch (error) {
      logError("Failed to save auto_check_update:", error);
    }
  };



  return (
    <>
      <div className="space-y-4">
        {/* Startup Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("启动")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("配置应用启动行为")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("开机自启动")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("系统启动时自动运行")}
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
                  {t("以管理员身份启动")}
                  {settings.is_running_as_admin && (
                    <span className="text-[10px] px-1.5 py-0.5 bg-primary/10 text-primary rounded animate-in fade-in duration-200">
                      {t("当前已提权")}
                    </span>
                  )}
                </Label>
                <p className="text-xs text-muted-foreground">
                  {t("允许监听任务管理器等高权限窗口的点击")}
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
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("自动检查更新")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("仅在程序启动时自动检查更新")}
                </p>
              </div>
              <Switch
                checked={autoCheckUpdate}
                onCheckedChange={toggleAutoCheckUpdate}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("界面语言")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("切换应用界面显示语言")}
                </p>
              </div>
              <Select value={locale} onValueChange={(value) => void setLocale(value as typeof locale)}>
                <SelectTrigger className="w-[140px] h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {LANGUAGE_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {t(option.label)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </div>

        {/* Window Behavior Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("窗口")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("配置窗口显示行为")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("唤醒位置")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("窗口唤醒时的定位方式")}
                </p>
              </div>
              <Select
                value={settings.position_mode}
                onValueChange={(v) => changePositionMode(v as PositionMode)}
              >
                <SelectTrigger className="w-[140px] h-8 text-xs"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="follow_cursor">{t("跟随光标")}</SelectItem>
                  <SelectItem value="screen_center">{t("屏幕居中")}</SelectItem>
                  <SelectItem value="fixed_position">{t("上一次位置")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("记住窗口大小")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("启用后，手动拖拽调整的窗口大小将被保留")}
                </p>
              </div>
              <Switch checked={persistWindowSize} onCheckedChange={togglePersistWindowSize} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("自动重置状态")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("关闭窗口时重置搜索、分组筛选和滚动位置")}
                </p>
              </div>
              <Switch
                checked={autoResetState}
                onCheckedChange={setAutoResetState}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("入场动画")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("窗口显示时播放淡入缩放动画")}
                </p>
              </div>
              <Switch
                checked={windowAnimation}
                onCheckedChange={setWindowAnimation}
              />
            </div>
          </div>
        </div>

        {/* Search Bar Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("搜索栏")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("配置激活窗口时的搜索栏行为")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("默认聚焦")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("激活窗口时，默认聚焦搜索框")}
                </p>
              </div>
              <Switch
                checked={searchAutoFocus}
                onCheckedChange={setSearchAutoFocus}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("自动清除")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("激活窗口时，仅清空搜索框文字")}
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
          <h3 className="text-sm font-medium mb-3">{t("操作")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("配置交互与操作行为")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("粘贴后关闭窗口")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("点击卡片粘贴后自动关闭主窗口")}
                </p>
              </div>
              <Switch checked={pasteCloseWindow} onCheckedChange={setPasteCloseWindow} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("粘贴后置顶")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("成功粘贴后将该条目移动到列表顶部")}
                </p>
              </div>
              <Switch checked={pasteMoveToTop} onCheckedChange={setPasteMoveToTop} />
            </div>
          </div>
        </div>


        {/* Log Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("日志")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("调试与故障排查")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("保存日志到文件")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("日志文件上限 10MB，超出自动轮转")}
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
                {t("路径：{path}", { path: settings.log_file_path })}
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
              {pendingAdminLaunch ? t("启用管理员模式") : t("关闭管理员模式")}
            </DialogTitle>
            <DialogDescription>
              {t("此设置需要重启应用后才能生效")}
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
              {t("取消")}
            </Button>
            <Button
              variant="outline"
              onClick={async () => {
                if (pendingAdminLaunch !== null) {
                  try {
                    // 直接保存到后端
                    if (pendingAdminLaunch) {
                      await invoke("enable_admin_launch");
                    } else {
                      await invoke("disable_admin_launch");
                    }
                    onSettingsChange({ ...settings, admin_launch: pendingAdminLaunch });
                  } catch (error) {
                      alert(t("操作失败: {error}", { error: String(error) }));
                  }
                }
                setAdminRestartDialogOpen(false);
                setPendingAdminLaunch(null);
              }}
            >
              {t("稍后重启")}
            </Button>
            <Button
              onClick={async () => {
                if (pendingAdminLaunch !== null) {
                  try {
                    // 重启前保存到后端
                    if (pendingAdminLaunch) {
                      await invoke("enable_admin_launch");
                    } else {
                      await invoke("disable_admin_launch");
                    }
                    onSettingsChange({ ...settings, admin_launch: pendingAdminLaunch });
                    await invoke("restart_app");
                  } catch (error) {
                     alert(t("操作失败: {error}", { error: String(error) }));
                    setAdminRestartDialogOpen(false);
                    setPendingAdminLaunch(null);
                  }
                }
              }}
            >
              {t("立即重启")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Log Restart Dialog */}
      <Dialog open={logRestartDialogOpen} onOpenChange={setLogRestartDialogOpen}>
        <DialogContent className="max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {pendingLogToFile ? t("启用日志保存") : t("关闭日志保存")}
            </DialogTitle>
            <DialogDescription>
              {t("此设置需要重启应用后才能生效")}
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
              {t("取消")}
            </Button>
            <Button
              variant="outline"
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                  } catch (error) {
                     alert(t("操作失败: {error}", { error: String(error) }));
                  }
                }
                setLogRestartDialogOpen(false);
                setPendingLogToFile(null);
              }}
            >
              {t("稍后重启")}
            </Button>
            <Button
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                    await invoke("restart_app");
                  } catch (error) {
                     alert(t("操作失败: {error}", { error: String(error) }));
                    setLogRestartDialogOpen(false);
                    setPendingLogToFile(null);
                  }
                }
              }}
            >
              {t("立即重启")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
