import type { RefObject } from "react";
import {
  ArrowDown16Regular,
  ArrowSync16Regular,
  ArrowUp16Regular,
} from "@fluentui/react-icons";
import { Button } from "@/components/ui/button";
import type { SyncStatusType } from "@/hooks/useWebDAVActions";

type ManualSyncSectionProps = {
  syncing: boolean;
  url: string;
  lastSyncTime: string;
  statusMsg: string;
  statusType: SyncStatusType;
  statusMsgRef: RefObject<HTMLDivElement | null>;
  onUpload: () => void;
  onDownload: () => void;
};

export function ManualSyncSection({
  syncing,
  url,
  lastSyncTime,
  statusMsg,
  statusType,
  statusMsgRef,
  onUpload,
  onDownload,
}: ManualSyncSectionProps) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">手动同步</h3>
      <p className="text-xs text-muted-foreground mb-4">
        立即执行同步操作（覆盖远端文件，避免数据膨胀）
      </p>
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={onUpload}
            disabled={syncing || !url}
          >
            {syncing ? (
              <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
            ) : (
              <ArrowUp16Regular className="w-3.5 h-3.5 mr-1" />
            )}
            上传至云端
          </Button>
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={onDownload}
            disabled={syncing || !url}
          >
            {syncing ? (
              <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
            ) : (
              <ArrowDown16Regular className="w-3.5 h-3.5 mr-1" />
            )}
            下载至本地
          </Button>
        </div>

        {lastSyncTime && (
          <p className="text-xs text-muted-foreground">
            上次同步：{lastSyncTime}
          </p>
        )}

        {statusMsg && (
          <div
            ref={statusMsgRef}
            className={`text-xs px-3 py-2 rounded-md whitespace-pre-line ${
              statusType === "success"
                ? "bg-green-500/10 text-green-600 dark:text-green-400"
                : statusType === "error"
                  ? "bg-destructive/10 text-destructive"
                  : "bg-muted text-muted-foreground"
            }`}
          >
            {statusMsg}
          </div>
        )}
      </div>
    </div>
  );
}