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
import { LOCALE_OPTIONS, useTranslation, type Locale } from "@/i18n";
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
  const { t, locale, setLocale } = useTranslation();
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
  const [trayIconVisible, setTrayIconVisible] = useState(true);


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
    invoke<string | null>("get_setting", { key: "tray_icon_visible" })
      .then((v) => setTrayIconVisible(v !== "false"))
      .catch((error) => {
        logError("Failed to load tray_icon_visible:", error);
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

  const toggleTrayIconVisible = async (enabled: boolean) => {
    setTrayIconVisible(enabled);
    try {
      await invoke("set_tray_icon_visibility", { visible: enabled });
    } catch (error) {
      setTrayIconVisible(!enabled);
      logError("Failed to set tray icon visibility:", error);
    }
  };

  const openLogFile = async () => {
    try {
      await invoke("open_log_file");
    } catch (error) {
      logError("Failed to open log file:", error);
    }
  };

  return (
    <>
      <div className="space-y-4">
        <div className="rounded-lg border bg-card p-4">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("language.label")}</Label>
              <p className="text-xs text-muted-foreground">{t("language.desc")}</p>
            </div>
            <Select value={locale} onValueChange={(v) => void setLocale(v as Locale)}>
              <SelectTrigger className="w-[140px] h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                {LOCALE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {t(option.labelKey)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        {/* Startup Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("settings.general.startupTitle")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("settings.general.startupDesc")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.autoStart")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.autoStartDesc")}
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
                  {t("settings.general.adminLaunch")}
                  {settings.is_running_as_admin && (
                    <span className="text-[10px] px-1.5 py-0.5 bg-primary/10 text-primary rounded animate-in fade-in duration-200">
                      {t("settings.general.adminLaunchBadge")}
                    </span>
                  )}
                </Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.adminLaunchDesc")}
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
                <Label className="text-xs">{t("settings.general.autoCheckUpdate")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.autoCheckUpdateDesc")}
                </p>
              </div>
              <Switch
                checked={autoCheckUpdate}
                onCheckedChange={toggleAutoCheckUpdate}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.trayIcon")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.trayIconDesc")}
                </p>
              </div>
              <Switch
                checked={trayIconVisible}
                onCheckedChange={toggleTrayIconVisible}
              />
            </div>
          </div>
        </div>

        {/* Window Behavior Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("settings.general.windowTitle")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("settings.general.windowDesc")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.positionMode")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.positionModeDesc")}
                </p>
              </div>
              <Select
                value={settings.position_mode}
                onValueChange={(v) => changePositionMode(v as PositionMode)}
              >
                <SelectTrigger className="w-[140px] h-8 text-xs"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="follow_cursor">{t("settings.general.positionFollowCursor")}</SelectItem>
                  <SelectItem value="screen_center">{t("settings.general.positionScreenCenter")}</SelectItem>
                  <SelectItem value="fixed_position">{t("settings.general.positionFixed")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.persistSize")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.persistSizeDesc")}
                </p>
              </div>
              <Switch checked={persistWindowSize} onCheckedChange={togglePersistWindowSize} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.autoResetState")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.autoResetStateDesc")}
                </p>
              </div>
              <Switch
                checked={autoResetState}
                onCheckedChange={setAutoResetState}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.animation")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.animationDesc")}
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
          <h3 className="text-sm font-medium mb-3">{t("settings.general.searchTitle")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("settings.general.searchDesc")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.autoFocus")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.autoFocusDesc")}
                </p>
              </div>
              <Switch
                checked={searchAutoFocus}
                onCheckedChange={setSearchAutoFocus}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.autoClear")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.autoClearDesc")}
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
          <h3 className="text-sm font-medium mb-3">{t("settings.general.operationTitle")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("settings.general.operationDesc")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.pasteCloseWindow")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.pasteCloseWindowDesc")}
                </p>
              </div>
              <Switch checked={pasteCloseWindow} onCheckedChange={setPasteCloseWindow} />
            </div>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.pasteMoveToTop")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.pasteMoveToTopDesc")}
                </p>
              </div>
              <Switch checked={pasteMoveToTop} onCheckedChange={setPasteMoveToTop} />
            </div>
          </div>
        </div>


        {/* Log Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">{t("settings.general.logTitle")}</h3>
          <p className="text-xs text-muted-foreground mb-4">{t("settings.general.logDesc")}</p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.general.logToFile")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.general.logToFileDesc")}
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
                {t("settings.general.logFilePathLabel")}
                <button
                  type="button"
                  onClick={openLogFile}
                  className="text-primary hover:underline cursor-pointer"
                >
                  {settings.log_file_path}
                </button>
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
              {pendingAdminLaunch ? t("settings.general.adminEnableTitle") : t("settings.general.adminDisableTitle")}
            </DialogTitle>
            <DialogDescription>
              {t("common.requiresRestart")}
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
              {t("common.cancel")}
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
                    alert(t("common.operationFailed", { error: String(error) }));
                  }
                }
                setAdminRestartDialogOpen(false);
                setPendingAdminLaunch(null);
              }}
            >
              {t("common.restartLater")}
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
                    alert(t("common.operationFailed", { error: String(error) }));
                    setAdminRestartDialogOpen(false);
                    setPendingAdminLaunch(null);
                  }
                }
              }}
            >
              {t("common.restartNow")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {/* Log Restart Dialog */}
      <Dialog open={logRestartDialogOpen} onOpenChange={setLogRestartDialogOpen}>
        <DialogContent className="max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {pendingLogToFile ? t("settings.general.logEnableTitle") : t("settings.general.logDisableTitle")}
            </DialogTitle>
            <DialogDescription>
              {t("common.requiresRestart")}
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
              {t("common.cancel")}
            </Button>
            <Button
              variant="outline"
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                  } catch (error) {
                    alert(t("common.operationFailed", { error: String(error) }));
                  }
                }
                setLogRestartDialogOpen(false);
                setPendingLogToFile(null);
              }}
            >
              {t("common.restartLater")}
            </Button>
            <Button
              onClick={async () => {
                if (pendingLogToFile !== null) {
                  try {
                    await invoke("set_log_to_file", { enabled: pendingLogToFile });
                    onSettingsChange({ ...settings, log_to_file: pendingLogToFile });
                    await invoke("restart_app");
                  } catch (error) {
                    alert(t("common.operationFailed", { error: String(error) }));
                    setLogRestartDialogOpen(false);
                    setPendingLogToFile(null);
                  }
                }
              }}
            >
              {t("common.restartNow")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
