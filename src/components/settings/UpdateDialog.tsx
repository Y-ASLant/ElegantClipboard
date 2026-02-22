import { useState, useEffect, useCallback } from "react";
import {
  ArrowDownload16Regular,
  ArrowSync16Regular,
  CheckmarkCircle16Regular,
  ErrorCircle16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { formatSize } from "@/lib/format";

// ── Types ──

interface UpdateInfo {
  has_update: boolean;
  latest_version: string;
  current_version: string;
  release_notes: string;
  download_url: string;
  file_name: string;
  file_size: number;
  published_at: string;
}

interface DownloadProgress {
  downloaded: number;
  total: number;
}

type UpdateStatus =
  | "checking"
  | "no-update"
  | "update-available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "error";

// ── UpdateDialog ──

interface UpdateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function UpdateDialog({ open, onOpenChange }: UpdateDialogProps) {
  const [status, setStatus] = useState<UpdateStatus>("checking");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState<DownloadProgress>({
    downloaded: 0,
    total: 0,
  });
  const [installerPath, setInstallerPath] = useState("");
  const [errorMsg, setErrorMsg] = useState("");

  const checkUpdate = useCallback(async () => {
    setStatus("checking");
    setErrorMsg("");
    setUpdateInfo(null);
    try {
      const info = await invoke<UpdateInfo>("check_for_update");
      setUpdateInfo(info);
      setStatus(info.has_update ? "update-available" : "no-update");
    } catch (e) {
      setErrorMsg(String(e));
      setStatus("error");
    }
  }, []);

  // Check for update when dialog opens; reset when closed
  useEffect(() => {
    if (open) {
      checkUpdate();
    } else {
      setStatus("checking");
      setUpdateInfo(null);
      setProgress({ downloaded: 0, total: 0 });
      setInstallerPath("");
      setErrorMsg("");
    }
  }, [open, checkUpdate]);

  // Listen for download progress events
  useEffect(() => {
    if (status !== "downloading") return;
    const unlisten = listen<DownloadProgress>(
      "update-download-progress",
      (event) => {
        setProgress(event.payload);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [status]);

  const startDownload = async () => {
    if (!updateInfo) return;
    setStatus("downloading");
    setProgress({ downloaded: 0, total: 0 });
    setErrorMsg("");
    try {
      const path = await invoke<string>("download_update", {
        downloadUrl: updateInfo.download_url,
        fileName: updateInfo.file_name,
      });
      setInstallerPath(path);
      setStatus("downloaded");
    } catch (e) {
      const msg = String(e);
      if (msg.includes("取消")) {
        setStatus("update-available");
      } else {
        setErrorMsg(msg);
        setStatus("error");
      }
    }
  };

  const cancelDownload = async () => {
    await invoke("cancel_update_download");
  };

  const installUpdate = async () => {
    if (!installerPath) return;
    setStatus("installing");
    try {
      await invoke("install_update", { installerPath });
    } catch (e) {
      setErrorMsg(String(e));
      setStatus("error");
    }
  };

  const progressPercent =
    progress.total > 0
      ? Math.round((progress.downloaded / progress.total) * 100)
      : 0;

  // Prevent closing during download or install
  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && (status === "downloading" || status === "installing"))
      return;
    onOpenChange(newOpen);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg" showCloseButton={status !== "downloading" && status !== "installing"}>
        <DialogHeader>
          <DialogTitle>检查更新</DialogTitle>
          {status === "update-available" && updateInfo && (
            <DialogDescription>
              v{updateInfo.current_version} → v{updateInfo.latest_version}
            </DialogDescription>
          )}
        </DialogHeader>

        {/* Checking */}
        {status === "checking" && (
          <div className="flex items-center justify-center gap-2 py-8">
            <ArrowSync16Regular className="w-5 h-5 text-primary animate-spin" />
            <span className="text-sm text-muted-foreground">
              正在检查更新...
            </span>
          </div>
        )}

        {/* No update */}
        {status === "no-update" && (
          <div className="flex flex-col items-center gap-2 py-8">
            <CheckmarkCircle16Regular className="w-8 h-8 text-primary" />
            <span className="text-sm font-medium">已是最新版本</span>
            <span className="text-xs text-muted-foreground">
              v{updateInfo?.current_version}
            </span>
          </div>
        )}

        {/* Update available */}
        {status === "update-available" && updateInfo && (
          <>
            {updateInfo.release_notes && (
              <div className="max-h-60 overflow-y-auto rounded-md border p-3">
                <SimpleMarkdown content={updateInfo.release_notes} />
              </div>
            )}
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted-foreground">
                {updateInfo.file_size > 0 && formatSize(updateInfo.file_size)}
              </span>
              <Button size="sm" onClick={startDownload}>
                <ArrowDownload16Regular className="w-4 h-4" />
                下载更新
              </Button>
            </div>
          </>
        )}

        {/* Downloading */}
        {status === "downloading" && (
          <div className="space-y-3 py-4">
            <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-primary rounded-full transition-all duration-200"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>
                {formatSize(progress.downloaded)} /{" "}
                {formatSize(progress.total)}
              </span>
              <span>{progressPercent}%</span>
            </div>
            <div className="flex justify-center pt-1">
              <Button variant="outline" size="sm" onClick={cancelDownload}>
                取消更新
              </Button>
            </div>
          </div>
        )}

        {/* Downloaded */}
        {status === "downloaded" && (
          <div className="flex flex-col items-center gap-3 py-4">
            <CheckmarkCircle16Regular className="w-8 h-8 text-primary" />
            <span className="text-sm font-medium">下载完成</span>
            <Button onClick={installUpdate}>安装并重启</Button>
          </div>
        )}

        {/* Installing */}
        {status === "installing" && (
          <div className="flex items-center justify-center gap-2 py-8">
            <ArrowSync16Regular className="w-5 h-5 text-primary animate-spin" />
            <span className="text-sm text-muted-foreground">
              正在启动安装程序...
            </span>
          </div>
        )}

        {/* Error */}
        {status === "error" && (
          <div className="flex flex-col items-center gap-3 py-4">
            <ErrorCircle16Regular className="w-8 h-8 text-destructive" />
            <span className="text-sm text-destructive text-center">
              {errorMsg}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={updateInfo ? startDownload : checkUpdate}
            >
              重试
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── Simple Markdown Renderer ──

/** Convert a subset of Markdown (headings, lists, bold, links) to HTML. */
function mdToHtml(md: string): string {
  return md
    .split("\n")
    .filter((l) => !l.trim().startsWith("<")) // strip raw HTML (e.g. <img>)
    .join("\n")
    .replace(/^### (.+)$/gm, '<h4 class="font-medium text-xs mt-3 mb-1 text-foreground">$1</h4>')
    .replace(/^## (.+)$/gm, '<h3 class="font-semibold text-sm mt-3 mb-1 text-foreground">$1</h3>')
    .replace(/^[*-] (.+)$/gm, '<li class="text-xs text-muted-foreground ml-3">$1</li>')
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="text-primary hover:underline">$1</a>')
    .replace(/(?<!href=")(https?:\/\/[^\s<>"']+)/g, '<a href="$1" class="text-primary hover:underline">$1</a>')
    .replace(/@([\w-]+)/g, '<a href="https://github.com/$1" class="text-primary hover:underline">@$1</a>')
    .replace(/\n{2,}/g, "<br/>")
    .replace(/\n/g, "");
}

function SimpleMarkdown({ content }: { content: string }) {
  if (!content) return null;
  return (
    <div
      className="text-xs text-muted-foreground leading-relaxed space-y-0.5"
      onClick={(e) => {
        const a = (e.target as HTMLElement).closest("a");
        if (a) { e.preventDefault(); openUrl(a.getAttribute("href")!); }
      }}
      dangerouslySetInnerHTML={{ __html: mdToHtml(content) }}
    />
  );
}
