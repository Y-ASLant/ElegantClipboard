import { useState } from "react";
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
  const [adminRestartDialogOpen, setAdminRestartDialogOpen] = useState(false);
  const [pendingAdminLaunch, setPendingAdminLaunch] = useState<boolean | null>(null);
  const [logRestartDialogOpen, setLogRestartDialogOpen] = useState(false);
  const [pendingLogToFile, setPendingLogToFile] = useState<boolean | null>(null);

  return (
    <>
      <div className="space-y-4">
        {/* Window Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">窗口</h3>
          <p className="text-xs text-muted-foreground mb-4">配置窗口显示行为</p>
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
              <Label className="text-xs">自动重置状态</Label>
              <p className="text-xs text-muted-foreground">
                关闭窗口时自动清空搜索并回到顶部
              </p>
            </div>
            <Switch
              checked={autoResetState}
              onCheckedChange={setAutoResetState}
            />
          </div>
        </div>

        {/* Startup Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">启动</h3>
          <p className="text-xs text-muted-foreground mb-4">配置应用启动行为</p>
          <div className="space-y-4">
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
        {/* Log Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">日志</h3>
          <p className="text-xs text-muted-foreground mb-4">调试与故障排查</p>
          <div className="space-y-4">
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
