import { useState } from "react";
import { Folder16Regular, Open16Regular } from "@fluentui/react-icons";
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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export interface DataSettings {
  data_path: string;
  max_history_count: number;
  max_content_size_kb: number;
}

interface DataTabProps {
  settings: DataSettings;
  onSettingsChange: (settings: DataSettings) => void;
}

interface MigrationResult {
  db_migrated: boolean;
  images_migrated: boolean;
  files_copied: number;
  bytes_copied: number;
  errors: string[];
}

export function DataTab({ settings, onSettingsChange }: DataTabProps) {
  const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
  const [pendingPath, setPendingPath] = useState<string | null>(null);
  const [migrating, setMigrating] = useState(false);
  const [migrationError, setMigrationError] = useState<string | null>(null);

  const selectFolder = async () => {
    try {
      const path = await invoke<string | null>("select_folder_for_settings");
      if (path && path !== settings.data_path) {
        // Check if there's existing data to migrate
        const currentPath = await invoke<string>("get_default_data_path");
        if (currentPath && currentPath !== path) {
          setPendingPath(path);
          setMigrationError(null);
          setMigrationDialogOpen(true);
        } else {
          // No migration needed, just set the path
          await invoke("set_data_path", { path });
          onSettingsChange({ ...settings, data_path: path });
        }
      }
    } catch (error) {
      console.error("Failed to select folder:", error);
    }
  };

  const handleMigrate = async () => {
    if (!pendingPath) return;
    
    setMigrating(true);
    setMigrationError(null);
    
    try {
      const result = await invoke<MigrationResult>("migrate_data_to_path", { 
        newPath: pendingPath 
      });
      
      if (result.errors.length > 0) {
        setMigrationError(`迁移完成但有错误: ${result.errors.join(", ")}`);
      } else {
        // Success - restart app
        setMigrationDialogOpen(false);
        onSettingsChange({ ...settings, data_path: pendingPath });
        await invoke("restart_app");
      }
    } catch (error) {
      setMigrationError(`迁移失败: ${error}`);
    } finally {
      setMigrating(false);
    }
  };

  const handleSkipMigration = async () => {
    if (!pendingPath) return;
    
    try {
      // Just set the new path without migrating
      await invoke("set_data_path", { path: pendingPath });
      onSettingsChange({ ...settings, data_path: pendingPath });
      setMigrationDialogOpen(false);
      // Restart to use new path
      await invoke("restart_app");
    } catch (error) {
      setMigrationError(`设置失败: ${error}`);
    }
  };

  const openDataFolder = async () => {
    try {
      await invoke("open_data_folder");
    } catch (error) {
      console.error("Failed to open folder:", error);
    }
  };

  const resetToDefault = async () => {
    try {
      const defaultPath = await invoke<string>("get_original_default_path");
      if (defaultPath !== settings.data_path) {
        setPendingPath(defaultPath);
        setMigrationError(null);
        setMigrationDialogOpen(true);
      }
    } catch (error) {
      console.error("Failed to reset path:", error);
    }
  };

  return (
    <>
      <div className="space-y-4">
        {/* Storage Path Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">数据存储</h3>
          <p className="text-xs text-muted-foreground mb-4">配置剪贴板数据的存储位置</p>
          <div className="space-y-2">
            <Label htmlFor="data-path" className="text-xs">存储路径</Label>
            <div className="flex gap-2">
              <Input
                id="data-path"
                value={settings.data_path}
                placeholder="加载中..."
                readOnly
                className="flex-1 h-8 text-sm path-text"
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="outline" size="icon" onClick={selectFolder} className="h-8 w-8">
                    <Folder16Regular className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>选择文件夹</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="outline" size="icon" onClick={openDataFolder} className="h-8 w-8">
                    <Open16Regular className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>打开文件夹</TooltipContent>
              </Tooltip>
            </div>
            <div className="flex items-center justify-between">
              <p className="text-xs text-muted-foreground">
                修改路径将迁移数据并重启应用
              </p>
              <button
                onClick={resetToDefault}
                className="text-xs text-primary hover:underline"
              >
                恢复默认
              </button>
            </div>
          </div>
        </div>

        {/* History Limit Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">历史记录</h3>
          <p className="text-xs text-muted-foreground mb-4">配置历史记录的存储限制</p>
          
          <div className="space-y-4">
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">最大历史记录数</Label>
                <span className="text-xs font-medium tabular-nums">
                  {settings.max_history_count === 0 ? "无限制" : settings.max_history_count.toLocaleString()}
                </span>
              </div>
              <Slider
                value={[settings.max_history_count]}
                onValueChange={(value) => onSettingsChange({ ...settings, max_history_count: value[0] })}
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
                  {settings.max_content_size_kb === 0 
                    ? "无限制"
                    : settings.max_content_size_kb >= 1024 
                      ? `${(settings.max_content_size_kb / 1024).toFixed(1)} MB`
                      : `${settings.max_content_size_kb} KB`
                  }
                </span>
              </div>
              <Slider
                value={[settings.max_content_size_kb]}
                onValueChange={(value) => onSettingsChange({ ...settings, max_content_size_kb: value[0] })}
                min={0}
                max={10240}
                step={64}
              />
              <p className="text-xs text-muted-foreground">
                设为 0 表示无限制
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Migration Confirmation Dialog */}
      <Dialog open={migrationDialogOpen} onOpenChange={setMigrationDialogOpen}>
        <DialogContent className="max-w-md" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>迁移数据</DialogTitle>
            <DialogDescription>
              是否将现有数据迁移到新位置？
            </DialogDescription>
          </DialogHeader>
          
          <div className="space-y-3 py-2">
            <div className="text-sm">
              <span className="text-muted-foreground">当前位置：</span>
              <span className="path-text text-xs block mt-1 p-2 bg-muted rounded">
                {settings.data_path}
              </span>
            </div>
            <div className="text-sm">
              <span className="text-muted-foreground">新位置：</span>
              <span className="path-text text-xs block mt-1 p-2 bg-muted rounded">
                {pendingPath}
              </span>
            </div>
            
            {migrationError && (
              <p className="text-sm text-destructive">{migrationError}</p>
            )}
          </div>
          
          <DialogFooter className="flex-col sm:flex-row gap-2">
            <Button
              variant="outline"
              onClick={() => setMigrationDialogOpen(false)}
              disabled={migrating}
            >
              取消
            </Button>
            <Button
              variant="ghost"
              onClick={handleSkipMigration}
              disabled={migrating}
              className="text-destructive hover:text-destructive hover:bg-destructive/10"
            >
              删除数据
            </Button>
            <Button
              onClick={handleMigrate}
              disabled={migrating}
            >
              {migrating ? "迁移中..." : "保留数据"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
