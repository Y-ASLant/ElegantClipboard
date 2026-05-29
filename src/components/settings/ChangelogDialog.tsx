import { useState, useEffect, useCallback } from "react";
import {
  ArrowSync16Regular,
  ErrorCircle16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SimpleMarkdown } from "@/components/settings/UpdateDialog";

interface VersionReleaseNotes {
  version: string;
  release_notes: string;
  published_at: string;
}

type ChangelogStatus = "loading" | "ready" | "error";

interface ChangelogDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  version: string;
}

export function ChangelogDialog({
  open,
  onOpenChange,
  version,
}: ChangelogDialogProps) {
  const [status, setStatus] = useState<ChangelogStatus>("loading");
  const [notes, setNotes] = useState<VersionReleaseNotes | null>(null);
  const [errorMsg, setErrorMsg] = useState("");

  const loadNotes = useCallback(async () => {
    setStatus("loading");
    setErrorMsg("");
    setNotes(null);
    try {
      const result = await invoke<VersionReleaseNotes>(
        "get_version_release_notes",
        { version },
      );
      setNotes(result);
      setStatus("ready");
    } catch (e) {
      setErrorMsg(String(e));
      setStatus("error");
    }
  }, [version]);

  useEffect(() => {
    if (!open) return;
    loadNotes();
  }, [open, loadNotes]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(96vw,1100px)] max-w-[1100px] max-h-[90vh] overflow-hidden">
        <DialogHeader>
          <DialogTitle>v{version} 更新日志</DialogTitle>
          {notes?.published_at && (
            <DialogDescription>
              发布于 {new Date(notes.published_at).toLocaleDateString("zh-CN")}
            </DialogDescription>
          )}
        </DialogHeader>

        {status === "loading" && (
          <div className="flex items-center justify-center gap-2 py-8">
            <ArrowSync16Regular className="w-5 h-5 text-primary animate-spin" />
            <span className="text-sm text-muted-foreground">正在加载...</span>
          </div>
        )}

        {status === "ready" && (
          <div className="max-h-[64vh] overflow-y-auto">
            {notes?.release_notes ? (
              <SimpleMarkdown content={notes.release_notes} />
            ) : (
              <p className="text-sm text-muted-foreground py-4 text-center">
                该版本暂无更新说明
              </p>
            )}
          </div>
        )}

        {status === "error" && (
          <div className="flex flex-col items-center gap-3 py-4">
            <ErrorCircle16Regular className="w-8 h-8 text-destructive" />
            <span className="text-sm text-destructive text-center">
              {errorMsg}
            </span>
            <Button variant="outline" size="sm" onClick={loadNotes}>
              重试
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
