import { useState, useEffect, useCallback } from "react";
import { Folder16Regular, Open16Regular, ArrowSync16Regular, ArrowDownload16Regular, ArrowUpload16Regular } from "@fluentui/react-icons";
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
import { logError } from "@/lib/logger";

export interface DataSettings {
  data_path: string;
  max_history_count: number;
  max_content_size_kb: number;
  auto_cleanup_days: number;
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

interface DataSizeInfo {
  db_size: number;
  images_size: number;
  images_count: number;
  total_size: number;
}

function formatDataSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

type DedupStrategy = "move_to_top" | "ignore" | "always_new";

const dedupOptions: { value: DedupStrategy; label: string; desc: string }[] = [
  { value: "move_to_top", label: "置顶已有", desc: "相同内容移到最前" },
  { value: "ignore", label: "忽略", desc: "不记录重复内容" },
  { value: "always_new", label: "总是新建", desc: "每次都创建新条目" },
];

function DedupStrategyCard() {
  const [strategy, setStrategy] = useState<DedupStrategy>("move_to_top");

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "dedup_strategy" }).then((val) => {
      if (val === "ignore" || val === "always_new") setStrategy(val);
    }).catch(() => {});
  }, []);

  const handleChange = async (value: DedupStrategy) => {
    setStrategy(value);
    try {
      await invoke("set_setting", { key: "dedup_strategy", value });
    } catch (error) {
      logError("Failed to save dedup strategy:", error);
    }
  };

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">重复内容处理</h3>
      <p className="text-xs text-muted-foreground mb-4">复制相同内容时的处理方式</p>
      <div className="flex gap-1">
        {dedupOptions.map((opt) => (
          <button
            key={opt.value}
            onClick={() => handleChange(opt.value)}
            className={`flex-1 px-2 py-1.5 text-xs rounded-md border transition-colors ${
              strategy === opt.value
                ? "bg-primary text-primary-foreground border-primary"
                : "bg-background text-foreground border-input hover:bg-accent"
            }`}
            title={opt.desc}
          >
            {opt.label}
          </button>
        ))}
      </div>
      <p className="text-xs text-muted-foreground mt-2">
        {dedupOptions.find((o) => o.value === strategy)?.desc}
      </p>
    </div>
  );
}

export function DataTab({ settings, onSettingsChange }: DataTabProps) {
  const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
  const [pendingPath, setPendingPath] = useState<string | null>(null);
  const [destHasData, setDestHasData] = useState(false);
  const [migrating, setMigrating] = useState(false);
  const [migrationError, setMigrationError] = useState<string | null>(null);
  const [dataSize, setDataSize] = useState<DataSizeInfo | null>(() => {
    try {
      const cached = sessionStorage.getItem("data-size-cache");
      return cached ? JSON.parse(cached).info : null;
    } catch { return null; }
  });
  const [dataSizeTime, setDataSizeTime] = useState<string | null>(() => {
    try {
      const cached = sessionStorage.getItem("data-size-cache");
      return cached ? JSON.parse(cached).time : null;
    } catch { return null; }
  });
  const [dataSizeLoading, setDataSizeLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [exportImportMsg, setExportImportMsg] = useState<string | null>(null);

  const refreshDataSize = useCallback(async () => {
    setDataSizeLoading(true);
    try {
      const info = await invoke<DataSizeInfo>("get_data_size");
      const time = new Date().toLocaleTimeString();
      setDataSize(info);
      setDataSizeTime(time);
      sessionStorage.setItem("data-size-cache", JSON.stringify({ info, time }));
    } catch { /* ignore */ }
    setDataSizeLoading(false);
  }, []);

  // 进入页面时自动加载数据统计（无缓存时）
  useEffect(() => {
    if (!dataSize) {
      refreshDataSize();
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const selectFolder = async () => {
    try {
      const path = await invoke<string | null>("select_folder_for_settings");
      if (path && path !== settings.data_path) {
        // Check if there's existing data to migrate
        const currentPath = await invoke<string>("get_default_data_path");
        if (currentPath && currentPath !== path) {
          const hasData = await invoke<boolean>("check_path_has_data", { path });
          setPendingPath(path);
          setDestHasData(hasData);
          setMigrationError(null);
          setMigrationDialogOpen(true);
        } else {
          // No migration needed, just set the path
          await invoke("set_data_path", { path });
          onSettingsChange({ ...settings, data_path: path });
        }
      }
    } catch (error) {
      logError("Failed to select folder:", error);
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
      // 如果目标已有数据（选择保留新位置数据），清理旧位置的数据
      if (destHasData) {
        await invoke("cleanup_data_at_path", { path: settings.data_path });
      }
      // Set the new path without migrating
      await invoke("set_data_path", { path: pendingPath });
      onSettingsChange({ ...settings, data_path: pendingPath });
      setMigrationDialogOpen(false);
      // Restart to use new path
      await invoke("restart_app");
    } catch (error) {
      setMigrationError(`设置失败: ${error}`);
    }
  };

  const handleExport = async () => {
    setExporting(true);
    setExportImportMsg(null);
    try {
      const msg = await invoke<string>("export_data");
      setExportImportMsg(msg);
    } catch (error) {
      const errStr = `${error}`;
      if (!errStr.includes("取消")) {
        setExportImportMsg(`导出失败: ${error}`);
      }
    } finally {
      setExporting(false);
    }
  };

  const handleImport = async () => {
    setImporting(true);
    setExportImportMsg(null);
    try {
      const msg = await invoke<string>("import_data");
      setExportImportMsg(msg);
      // 导入成功后重启应用
      await invoke("restart_app");
    } catch (error) {
      const errStr = `${error}`;
      if (!errStr.includes("取消")) {
        setExportImportMsg(`导入失败: ${error}`);
      }
    } finally {
      setImporting(false);
    }
  };

  const openDataFolder = async () => {
    try {
      await invoke("open_data_folder");
    } catch (error) {
      logError("Failed to open folder:", error);
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
      logError("Failed to reset path:", error);
    }
  };

  return (
    <>
      <div className="space-y-4">
        {/* Data Size Card */}
        <div className="rounded-lg border bg-card p-4">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-sm font-medium">数据统计</h3>
            <div className="flex items-center gap-2">
              {dataSizeTime && (
                <span className="text-xs text-muted-foreground">更新于 {dataSizeTime}</span>
              )}
              <Button
                variant="ghost"
                size="icon"
                onClick={refreshDataSize}
                disabled={dataSizeLoading}
                className="h-6 w-6"
              >
                <ArrowSync16Regular className={`w-3.5 h-3.5 ${dataSizeLoading ? "animate-spin" : ""}`} />
              </Button>
            </div>
          </div>
          {dataSize ? (
            <div className="grid grid-cols-3 gap-3">
              <div className="text-center p-2 rounded-md bg-muted/50">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.total_size)}</p>
                <p className="text-xs text-muted-foreground">总大小</p>
              </div>
              <div className="text-center p-2 rounded-md bg-muted/50">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.db_size)}</p>
                <p className="text-xs text-muted-foreground">数据库</p>
              </div>
              <div className="text-center p-2 rounded-md bg-muted/50">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.images_size)}</p>
                <p className="text-xs text-muted-foreground">图片（{dataSize.images_count} 张）</p>
              </div>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">点击右上角刷新按钮查看数据大小</p>
          )}
        </div>

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

        {/* Export / Import Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">数据备份</h3>
          <p className="text-xs text-muted-foreground mb-4">导出或导入剪贴板数据（ZIP 格式）</p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleExport}
              disabled={exporting || importing}
              className="flex-1"
            >
              <ArrowUpload16Regular className="w-4 h-4 mr-1.5" />
              {exporting ? "导出中..." : "导出数据"}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleImport}
              disabled={exporting || importing}
              className="flex-1"
            >
              <ArrowDownload16Regular className="w-4 h-4 mr-1.5" />
              {importing ? "导入中..." : "导入数据"}
            </Button>
          </div>
          {exportImportMsg && (
            <p className={`text-xs mt-2 ${exportImportMsg.includes("失败") ? "text-destructive" : "text-muted-foreground"}`}>
              {exportImportMsg}
            </p>
          )}
        </div>

        {/* Dedup Strategy Card */}
        <DedupStrategyCard />

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
                <Label className="text-xs">单条文本最大大小</Label>
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
                仅限制文本/HTML/RTF，图片和文件不受此限制，设为 0 表示无限制
              </p>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">自动清理天数</Label>
                <span className="text-xs font-medium tabular-nums">
                  {settings.auto_cleanup_days === 0 ? "不自动清理" : `${settings.auto_cleanup_days} 天`}
                </span>
              </div>
              <Slider
                value={[settings.auto_cleanup_days]}
                onValueChange={(value) => onSettingsChange({ ...settings, auto_cleanup_days: value[0] })}
                min={0}
                max={365}
                step={5}
              />
              <p className="text-xs text-muted-foreground">
                自动删除超过指定天数的历史记录，设为 0 表示不自动清理
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Migration Confirmation Dialog */}
      <Dialog open={migrationDialogOpen} onOpenChange={setMigrationDialogOpen}>
        <DialogContent className="max-w-md" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{destHasData ? "目标位置已有数据" : "迁移数据"}</DialogTitle>
            <DialogDescription>
              {destHasData
                ? "新位置已存在剪贴板数据，请选择保留哪一份数据。"
                : "是否将现有数据迁移到新位置？"}
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
            {destHasData ? (
              <>
                <Button
                  variant="ghost"
                  onClick={handleMigrate}
                  disabled={migrating}
                  className="text-destructive hover:text-destructive hover:bg-destructive/10"
                >
                  {migrating ? "覆盖中..." : "保留旧位置数据"}
                </Button>
                <Button
                  onClick={handleSkipMigration}
                  disabled={migrating}
                >
                  保留新位置数据
                </Button>
              </>
            ) : (
              <>
                <Button
                  variant="ghost"
                  onClick={handleSkipMigration}
                  disabled={migrating}
                  className="text-destructive hover:text-destructive hover:bg-destructive/10"
                >
                  不迁移
                </Button>
                <Button
                  onClick={handleMigrate}
                  disabled={migrating}
                >
                  {migrating ? "迁移中..." : "迁移数据"}
                </Button>
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

