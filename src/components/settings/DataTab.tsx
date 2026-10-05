import { useState, useEffect, useCallback, useMemo } from "react";
import { Folder16Regular, Open16Regular, ArrowSync16Regular, ArrowDownload16Regular, ArrowUpload16Regular, Delete16Regular, ArrowCounterclockwise16Regular, ArrowClockwise16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
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
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { getOperationErrorMessage, reportUserError } from "@/lib/operation-feedback";

export interface DataSettings {
  data_path: string;
  max_history_count: number;
  max_content_size_kb: number;
  max_image_size_kb: number;
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
  staged_size?: number;
  staged_count?: number;
  total_size: number;
}

function formatDataSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

type DedupStrategy = "move_to_top" | "ignore" | "always_new";
type TextDedupMode = "semantic" | "strict";

const dedupKeys: { value: DedupStrategy; labelKey: string; descKey: string }[] = [
  { value: "move_to_top", labelKey: "settings.data.dedupMoveToTop", descKey: "settings.data.dedupMoveToTopDesc" },
  { value: "ignore", labelKey: "settings.data.dedupIgnore", descKey: "settings.data.dedupIgnoreDesc" },
  { value: "always_new", labelKey: "settings.data.dedupAlwaysNew", descKey: "settings.data.dedupAlwaysNewDesc" },
];
const textDedupKeys: { value: TextDedupMode; labelKey: string; descKey: string }[] = [
  { value: "semantic", labelKey: "settings.data.textDedupSemantic", descKey: "settings.data.textDedupSemanticDesc" },
  { value: "strict", labelKey: "settings.data.textDedupStrict", descKey: "settings.data.textDedupStrictDesc" },
];

interface DedupStrategyCardProps {
  strategy: DedupStrategy;
  onChange: (value: DedupStrategy) => void | Promise<void>;
}

function DedupStrategyCard({ strategy, onChange }: DedupStrategyCardProps) {
  const { t } = useTranslation();
  const dedupOptions = useMemo(
    () => dedupKeys.map((k) => ({ value: k.value, label: t(k.labelKey), desc: t(k.descKey) })),
    [t],
  );
  const activeDedupIndex = Math.max(
    0,
    dedupOptions.findIndex((opt) => opt.value === strategy),
  );

  return (
    <SettingsCard>
      <SettingsCardHeader
        title={t("settings.data.dedupTitle")}
        description={t("settings.data.dedupDesc")}
      />
      <div
        role="radiogroup"
        aria-label={t("settings.data.dedupAria")}
        className="relative rounded-md border bg-muted-surface-subtle p-1"
      >
        <div className="relative grid grid-cols-3">
          <div
            aria-hidden
            className="absolute inset-y-0 left-0 w-1/3 rounded-md bg-primary elevation-control will-change-transform transition-transform duration-200 ease-out"
            style={{ transform: `translateX(${activeDedupIndex * 100}%)` }}
          />
          {dedupOptions.map((opt) => {
            const isActive = strategy === opt.value;
            return (
              <button
                key={opt.value}
                type="button"
                role="radio"
                aria-checked={isActive}
                onClick={() => { void onChange(opt.value); }}
                className={`relative z-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition-surface ${
                  isActive
                    ? "text-primary-foreground"
                    : "text-foreground/80 hover:text-foreground"
                }`}

              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>
      <p className="text-xs text-muted-foreground mt-2">
        {dedupOptions.find((o) => o.value === strategy)?.desc}
      </p>
    </SettingsCard>
  );
}


function TextDedupModeCard({ dedupStrategy }: { dedupStrategy: DedupStrategy }) {
  const { t } = useTranslation();
  const textDedupModeOptions = useMemo(
    () => textDedupKeys.map((k) => ({ value: k.value, label: t(k.labelKey), desc: t(k.descKey) })),
    [t],
  );
  const [mode, setMode] = useState<TextDedupMode>("semantic");
  const dedupEnabled = dedupStrategy !== "always_new";
  const activeIndex = Math.max(
    0,
    textDedupModeOptions.findIndex((opt) => opt.value === mode),
  );

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "text_dedup_mode" }).then((val) => {
      if (val === "strict") setMode("strict");
      else setMode("semantic");
    }).catch((error) => {
      logError("Failed to load text dedup mode:", error);
    });
  }, []);

  const handleChange = async (value: TextDedupMode) => {
    setMode(value);
    try {
      await invoke("set_setting", { key: "text_dedup_mode", value });
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.saveSettings"), error, "Failed to save text dedup mode");
    }
  };

  return (
    <SettingsCard>
      <SettingsCardHeader
        title={t("settings.data.textDedupTitle")}
        description={t("settings.data.textDedupDesc")}
      />
      <div
        role="radiogroup"
        aria-label={t("settings.data.textDedupAria")}
        aria-disabled={!dedupEnabled}
        className={`relative rounded-md border p-1 ${dedupEnabled ? "bg-muted-surface-subtle" : "bg-muted-surface-faint opacity-70"}`}
      >
        <div className="relative grid grid-cols-2">
          <div
            aria-hidden
            className={`absolute inset-y-0 left-0 w-1/2 rounded-md elevation-control will-change-transform transition-transform duration-200 ease-out ${dedupEnabled ? "bg-primary" : "bg-muted-foreground/25"}`}
            style={{ transform: `translateX(${activeIndex * 100}%)` }}
          />
          {textDedupModeOptions.map((opt) => {
            const isActive = mode === opt.value;
            return (
              <button
                key={opt.value}
                type="button"
                role="radio"
                aria-checked={isActive}
                disabled={!dedupEnabled}
                onClick={() => { if (dedupEnabled) void handleChange(opt.value); }}
                className={`relative z-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition-surface ${
                  !dedupEnabled
                    ? "text-muted-foreground cursor-not-allowed"
                    : isActive
                    ? "text-primary-foreground"
                    : "text-foreground/80 hover:text-foreground"
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>
      <p className="text-xs text-muted-foreground mt-2">
        {dedupEnabled
          ? textDedupModeOptions.find((o) => o.value === mode)?.desc
          : t("settings.data.textDedupDisabled")}
      </p>
    </SettingsCard>
  );
}
function formatKB(kb: number, fractionDigits = 1, unlimitedLabel: string): string {
  if (kb === 0) return unlimitedLabel;
  if (kb >= 1024) return `${(kb / 1024).toFixed(fractionDigits)} MB`;
  return `${kb} KB`;
}

export function DataTab({ settings, onSettingsChange }: DataTabProps) {
  const { t } = useTranslation();
  const unlimitedLabel = t("common.unlimited");
  const noAutoCleanupLabel = t("common.noAutoCleanup");
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
  const [exportImportFailed, setExportImportFailed] = useState(false);
  const [dedupStrategy, setDedupStrategy] = useState<DedupStrategy>("move_to_top");

  // 数据清理
  type CleanAction = "clear_history" | "reset_settings" | "reset_all";
  const [cleanDialogAction, setCleanDialogAction] = useState<CleanAction | null>(null);
  const [cleanLoading, setCleanLoading] = useState(false);
  const [cleanMsg, setCleanMsg] = useState<string | null>(null);
  const [cleanFailed, setCleanFailed] = useState(false);

  const cleanActionConfig = useMemo(() => ({
    clear_history: {
      title: t("settings.data.clearHistoryDialogTitle"),
      description: t("settings.data.clearHistoryDesc"),
      warning: t("settings.data.clearHistoryDialogWarning"),
      buttonText: t("settings.data.clearHistoryConfirm"),
      command: "clear_all_history",
      needsRestart: false,
    },
    reset_settings: {
      title: t("settings.data.resetSettingsDialogTitle"),
      description: t("settings.data.resetSettingsDesc"),
      warning: t("settings.data.resetSettingsDialogWarning"),
      buttonText: t("settings.data.resetSettingsConfirm"),
      command: "reset_settings",
      needsRestart: true,
    },
    reset_all: {
      title: t("settings.data.resetAllDialogTitle"),
      description: t("settings.data.resetAllDesc"),
      warning: t("settings.data.resetAllDialogWarning"),
      buttonText: t("settings.data.resetAllConfirm"),
      command: "reset_all_data",
      needsRestart: true,
    },
  }), [t]);

  const handleCleanAction = async () => {
    if (!cleanDialogAction) return;
    const config = cleanActionConfig[cleanDialogAction];
    setCleanLoading(true);
    setCleanMsg(null);
    setCleanFailed(false);
    try {
      await invoke(config.command);
      setCleanDialogAction(null);
      if (config.needsRestart) {
        sessionStorage.removeItem("data-size-cache");
        await invoke("restart_app");
      } else {
        setCleanMsg(t("common.success"));
        await refreshDataSize();
      }
    } catch (error) {
      setCleanFailed(true);
      logError("Failed to clean application data", error);
      setCleanMsg(t("common.operationFailed", { error: getOperationErrorMessage(error) }));
    } finally {
      setCleanLoading(false);
    }
  };

  const refreshDataSize = useCallback(async (userRequested = false) => {
    setDataSizeLoading(true);
    try {
      const info = await invoke<DataSizeInfo>("get_data_size");
      const time = new Date().toLocaleTimeString();
      setDataSize(info);
      setDataSizeTime(time);
      sessionStorage.setItem("data-size-cache", JSON.stringify({ info, time }));
    } catch (error) {
      if (userRequested) reportUserError(t("operationFeedback.userActions.loadSize"), error, "Failed to refresh data size");
      else logError("Failed to refresh data size", error);
    }
    setDataSizeLoading(false);
  }, [t]);

  // 进入页面时自动加载数据统计（无缓存时）
  useEffect(() => {
    if (!dataSize) {
      refreshDataSize();
    }
  }, [refreshDataSize]);

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "dedup_strategy" })
      .then((val) => {
        if (val === "ignore" || val === "always_new") {
          setDedupStrategy(val);
        }
      })
      .catch((error) => {
        logError("Failed to load dedup strategy:", error);
      });
  }, []);

  const handleDedupStrategyChange = async (value: DedupStrategy) => {
    setDedupStrategy(value);
    try {
      await invoke("set_setting", { key: "dedup_strategy", value });
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.saveSettings"), error, "Failed to save dedup strategy");
    }
  };

  const selectFolder = async () => {
    try {
      const path = await invoke<string | null>("select_folder_for_settings");
      if (path && path !== settings.data_path) {
        // 检查是否有现有数据需要迁移
        const currentPath = await invoke<string>("get_default_data_path");
        if (currentPath && currentPath !== path) {
          const hasData = await invoke<boolean>("check_path_has_data", { path });
          setPendingPath(path);
          setDestHasData(hasData);
          setMigrationError(null);
          setMigrationDialogOpen(true);
        } else {
          // 无需迁移，直接设置路径
          await invoke("set_data_path", { path });
          onSettingsChange({ ...settings, data_path: path });
        }
      }
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.selectFolder"), error, "Failed to select data folder");
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
        logError("Data migration reported errors", result.errors);
        setMigrationError(t("settings.data.migrationErrors", { errors: t("operationFeedback.unknownReason") }));
      } else {
        // 成功，重启应用
        setMigrationDialogOpen(false);
        onSettingsChange({ ...settings, data_path: pendingPath });
        await invoke("restart_app");
      }
    } catch (error) {
      logError("Failed to migrate data", error);
      setMigrationError(t("settings.data.migrationFailed", { error: getOperationErrorMessage(error) }));
    } finally {
      setMigrating(false);
    }
  };

  const handleSkipMigration = async () => {
    if (!pendingPath) return;
    
    try {
      // 不删除旧位置数据：当前数据库连接未关闭，强制删除会触发 OS error 32。
      // 旧数据留在磁盘上无害，用户可手动清理。
      await invoke("set_data_path", { path: pendingPath });
      onSettingsChange({ ...settings, data_path: pendingPath });
      setMigrationDialogOpen(false);
      // 重启以使用新路径
      await invoke("restart_app");
    } catch (error) {
      logError("Failed to change data path", error);
      setMigrationError(t("settings.data.setPathFailed", { error: getOperationErrorMessage(error) }));
    }
  };

  const handleExport = async () => {
    setExporting(true);
    setExportImportMsg(null);
    setExportImportFailed(false);
    try {
      const msg = await invoke<string>("export_data");
      setExportImportMsg(msg);
    } catch (error) {
      if (error !== "用户取消了导出") {
        setExportImportFailed(true);
        logError("Failed to export data", error);
        setExportImportMsg(t("settings.data.exportFailed", { error: getOperationErrorMessage(error) }));
      }
    } finally {
      setExporting(false);
    }
  };

  const handleImport = async () => {
    setImporting(true);
    setExportImportMsg(null);
    setExportImportFailed(false);
    try {
      const msg = await invoke<string>("import_data");
      setExportImportMsg(msg);
      // 导入成功后重启应用
      await invoke("restart_app");
    } catch (error) {
      if (error !== "用户取消了导入") {
        setExportImportFailed(true);
        logError("Failed to import data", error);
        setExportImportMsg(t("settings.data.importFailed", { error: getOperationErrorMessage(error) }));
      }
    } finally {
      setImporting(false);
    }
  };

  const openDataFolder = async () => {
    try {
      await invoke("open_data_folder");
    } catch (error) {
      reportUserError(t("operationFeedback.operations.showInExplorer"), error, "Failed to open data folder");
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
      reportUserError(t("operationFeedback.userActions.saveSettings"), error, "Failed to reset data path");
    }
  };

  return (
    <>
      <div className="space-y-3">
        {/* Data Size Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.data.statsTitle")}
            action={
              <div className="flex items-center gap-2">
                {dataSizeTime && (
                  <span className="text-xs text-muted-foreground">{t("common.updatedAt", { time: dataSizeTime })}</span>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => { void refreshDataSize(true); }}
                  disabled={dataSizeLoading}
                  className="h-6 w-6"
                >
                  <ArrowSync16Regular className={`w-3.5 h-3.5 ${dataSizeLoading ? "animate-spin" : ""}`} />
                </Button>
              </div>
            }
          />
          {dataSize ? (
            <div className="grid grid-cols-3 gap-3">
              <div className="text-center p-2 rounded-md bg-muted-surface">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.total_size)}</p>
                <p className="text-xs text-muted-foreground">{t("settings.data.totalSize")}</p>
              </div>
              <div className="text-center p-2 rounded-md bg-muted-surface">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.db_size)}</p>
                <p className="text-xs text-muted-foreground">{t("settings.data.database")}</p>
              </div>
              <div className="text-center p-2 rounded-md bg-muted-surface">
                <p className="text-sm font-medium tabular-nums">{formatDataSize(dataSize.images_size)}</p>
                <p className="text-xs text-muted-foreground">{t("common.imagesCount", { count: dataSize.images_count })}</p>
              </div>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">{t("settings.data.statsRefreshHint")}</p>
          )}
        </SettingsCard>

        {/* Storage Path Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.data.storageTitle")}
            description={t("settings.data.storageDesc")}
          />
          <div className="space-y-2">
            <Label htmlFor="data-path" className="text-xs">{t("settings.data.storagePath")}</Label>
            <div className="flex gap-2">
              <Input
                id="data-path"
                value={settings.data_path}
                placeholder={t("settings.data.loadingPath")}
                readOnly
                className="flex-1 h-8 text-sm path-text"
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="outline" size="icon" onClick={selectFolder} className="h-8 w-8">
                    <Folder16Regular className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("settings.data.selectFolder")}</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="outline" size="icon" onClick={openDataFolder} className="h-8 w-8">
                    <Open16Regular className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("settings.data.openFolder")}</TooltipContent>
              </Tooltip>
            </div>
            <div className="flex items-center justify-between">
              <p className="text-xs text-muted-foreground">
                {t("settings.data.migrateHint")}
              </p>
              <button
                onClick={resetToDefault}
                className="text-xs text-primary hover:underline"
              >
                {t("settings.data.restoreDefault")}
              </button>
            </div>
          </div>
        </SettingsCard>

        {/* Export / Import Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.data.backupTitle")}
            description={t("settings.data.backupDesc")}
          />
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleExport}
              disabled={exporting || importing}
              className="flex-1"
            >
              <ArrowUpload16Regular className="w-4 h-4 mr-1.5" />
              {exporting ? t("settings.data.exporting") : t("settings.data.exportData")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleImport}
              disabled={exporting || importing}
              className="flex-1"
            >
              <ArrowDownload16Regular className="w-4 h-4 mr-1.5" />
              {importing ? t("settings.data.importing") : t("settings.data.importData")}
            </Button>
          </div>
          {exportImportMsg && (
            <p role={exportImportFailed ? "alert" : "status"} className={`text-xs mt-2 ${exportImportFailed ? "text-destructive" : "text-muted-foreground"}`}>
              {exportImportMsg}
            </p>
          )}
        </SettingsCard>

        {/* Data Cleanup Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.data.cleanupTitle")}
            description={t("settings.data.cleanupDesc")}
          />
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <p className="text-sm">{t("settings.data.clearHistory")}</p>
                <p className="text-xs text-muted-foreground">{t("settings.data.clearHistoryDesc")}</p>
              </div>
              <Button
                variant="destructive"
                size="sm"
                className="shrink-0"
                onClick={() => { setCleanMsg(null); setCleanDialogAction("clear_history"); }}
              >
                <Delete16Regular className="w-4 h-4 mr-1.5" />
                {t("settings.data.clearHistoryBtn")}
              </Button>
            </div>
            <div className="h-px bg-border" />
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <p className="text-sm">{t("settings.data.resetSettings")}</p>
                <p className="text-xs text-muted-foreground">{t("settings.data.resetSettingsDesc")}</p>
              </div>
              <Button
                variant="destructive"
                size="sm"
                className="shrink-0"
                onClick={() => { setCleanMsg(null); setCleanDialogAction("reset_settings"); }}
              >
                <ArrowCounterclockwise16Regular className="w-4 h-4 mr-1.5" />
                {t("settings.data.resetSettingsBtn")}
              </Button>
            </div>
            <div className="h-px bg-border" />
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <p className="text-sm">{t("settings.data.resetAll")}</p>
                <p className="text-xs text-muted-foreground">{t("settings.data.resetAllDesc")}</p>
              </div>
              <Button
                variant="destructive"
                size="sm"
                className="shrink-0"
                onClick={() => { setCleanMsg(null); setCleanDialogAction("reset_all"); }}
              >
                <ArrowClockwise16Regular className="w-4 h-4 mr-1.5" />
                {t("settings.data.resetAllBtn")}
              </Button>
            </div>
          </div>
          {cleanMsg && !cleanDialogAction && (
            <p role={cleanFailed ? "alert" : "status"} className={`text-xs mt-3 ${cleanFailed ? "text-destructive" : "text-muted-foreground"}`}>
              {cleanMsg}
            </p>
          )}
        </SettingsCard>

        {/* Dedup Strategy Card */}
        <DedupStrategyCard strategy={dedupStrategy} onChange={handleDedupStrategyChange} />
        <TextDedupModeCard dedupStrategy={dedupStrategy} />

        {/* History Limit Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.data.historyTitle")}
            description={t("settings.data.historyDesc")}
          />
          
          <div className="space-y-4">
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.data.maxHistory")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {settings.max_history_count === 0 ? unlimitedLabel : settings.max_history_count.toLocaleString()}
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
                {t("settings.data.maxHistoryHint")}
              </p>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.data.maxTextSize")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {formatKB(settings.max_content_size_kb, 1, unlimitedLabel)}
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
                {t("settings.data.maxTextSizeHint")}
              </p>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.data.maxImageSize")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {formatKB(settings.max_image_size_kb, 0, unlimitedLabel)}
                </span>
              </div>
              <Slider
                value={[settings.max_image_size_kb]}
                onValueChange={(value) => onSettingsChange({ ...settings, max_image_size_kb: value[0] })}
                min={0}
                max={512000}
                step={1024}
              />
              <p className="text-xs text-muted-foreground">
                {t("settings.data.maxImageSizeHint")}
              </p>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.data.autoCleanup")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {settings.auto_cleanup_days === 0 ? noAutoCleanupLabel : t("common.days", { count: settings.auto_cleanup_days })}
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
                {t("settings.data.autoCleanupHint")}
              </p>
            </div>
          </div>
        </SettingsCard>
      </div>

      {/* Data Cleanup Confirmation Dialog */}
      <Dialog open={cleanDialogAction !== null} onOpenChange={(open) => { if (!open && !cleanLoading) setCleanDialogAction(null); }}>
        <DialogContent className="max-w-md" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {cleanDialogAction ? cleanActionConfig[cleanDialogAction].title : ""}
            </DialogTitle>
            <DialogDescription>
              {cleanDialogAction ? cleanActionConfig[cleanDialogAction].warning : ""}
            </DialogDescription>
          </DialogHeader>
          {cleanMsg && (
            <p role="alert" className="text-sm text-destructive">{cleanMsg}</p>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setCleanDialogAction(null)}
              disabled={cleanLoading}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={handleCleanAction}
              disabled={cleanLoading}
            >
              {cleanLoading
                ? t("common.processing")
                : (cleanDialogAction ? cleanActionConfig[cleanDialogAction].buttonText : "")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Migration Confirmation Dialog */}
      <Dialog open={migrationDialogOpen} onOpenChange={setMigrationDialogOpen}>
        <DialogContent className="max-w-md" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{destHasData ? t("settings.data.migrationDestHasData") : t("settings.data.migrationTitle")}</DialogTitle>
            <DialogDescription>
              {destHasData
                ? t("settings.data.migrationDestDesc")
                : t("settings.data.migrationAsk")}
            </DialogDescription>
          </DialogHeader>
          
          <div className="space-y-3 py-2">
            <div className="text-sm">
              <span className="text-muted-foreground">{t("settings.data.currentPath")}</span>
              <span className="path-text text-xs block mt-1 p-2 bg-muted rounded-md">
                {settings.data_path}
              </span>
            </div>
            <div className="text-sm">
              <span className="text-muted-foreground">{t("settings.data.newPath")}</span>
              <span className="path-text text-xs block mt-1 p-2 bg-muted rounded-md">
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
              {t("common.cancel")}
            </Button>
            {destHasData ? (
              <>
                <Button
                  variant="ghost"
                  onClick={handleMigrate}
                  disabled={migrating}
                  className="text-destructive hover:text-destructive hover:bg-destructive-subtle-hover"
                >
                  {migrating ? t("settings.data.overwriting") : t("settings.data.keepOldData")}
                </Button>
                <Button
                  onClick={handleSkipMigration}
                  disabled={migrating}
                >
                  {t("settings.data.keepNewData")}
                </Button>
              </>
            ) : (
              <>
                <Button
                  variant="ghost"
                  onClick={handleSkipMigration}
                  disabled={migrating}
                  className="text-destructive hover:text-destructive hover:bg-destructive-subtle-hover"
                >
                  {t("settings.data.skipMigration")}
                </Button>
                <Button
                  onClick={handleMigrate}
                  disabled={migrating}
                >
                  {migrating ? t("settings.data.migrating") : t("settings.data.migrateData")}
                </Button>
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

