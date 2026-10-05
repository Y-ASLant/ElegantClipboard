import { useState, useEffect, useCallback, useMemo, useRef, type ReactNode } from "react";
import {
  ArrowDownload16Regular,
  ArrowSync16Regular,
  CheckmarkCircle16Regular,
  ChevronDown16Regular,
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
import { t, useTranslation } from "@/i18n";
import { formatSize } from "@/lib/format";
import { logError } from "@/lib/logger";
import { getOperationErrorMessage, reportUserError } from "@/lib/operation-feedback";

// ── 类型定义 ──

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

interface ReleaseNotesSection {
  id: string;
  version: string;
  content: string;
}

function splitReleaseNotesByVersion(content: string, defaultVersionLabel: string): ReleaseNotesSection[] {
  const normalized = content.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const sections: ReleaseNotesSection[] = [];
  let currentVersion = "";
  let buffer: string[] = [];

  const pushSection = () => {
    const body = buffer.join("\n").trim();
    if (!currentVersion && !body) return;
    const version = currentVersion || defaultVersionLabel;
    sections.push({
      id: `${version}-${sections.length}`,
      version,
      content: body,
    });
  };

  for (const rawLine of lines) {
    const line = rawLine.trim();
    const headerMatch = line.match(/^##\s+v?([^\s].*)$/i);
    if (headerMatch) {
      pushSection();
      currentVersion = headerMatch[1].trim();
      buffer = [];
      continue;
    }
    buffer.push(rawLine);
  }

  pushSection();
  return sections;
}

// ── 更新对话框 ──

interface UpdateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function UpdateDialog({ open, onOpenChange }: UpdateDialogProps) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<UpdateStatus>("checking");
  const downloadCancelled = useRef(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState<DownloadProgress>({
    downloaded: 0,
    total: 0,
  });
  const [installerPath, setInstallerPath] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const [expandedSectionId, setExpandedSectionId] = useState("");

  const releaseSections = useMemo(
    () => splitReleaseNotesByVersion(updateInfo?.release_notes ?? "", t("settings.update.updateContent")),
    [updateInfo?.release_notes, t],
  );
  const outdatedVersionCount = releaseSections.length;

  useEffect(() => {
    if (status !== "update-available" || releaseSections.length === 0) {
      setExpandedSectionId("");
      return;
    }

    setExpandedSectionId((prev) =>
      releaseSections.some((section) => section.id === prev)
        ? prev
        : releaseSections[0].id,
    );
  }, [status, releaseSections]);

  const checkUpdate = useCallback(async () => {
    setStatus("checking");
    setErrorMsg("");
    setUpdateInfo(null);
    try {
      const info = await invoke<UpdateInfo>("check_for_update");
      setUpdateInfo(info);
      setStatus(info.has_update ? "update-available" : "no-update");
    } catch (e) {
      logError("Failed to check for updates", e);
      setErrorMsg(getOperationErrorMessage(e));
      setStatus("error");
    }
  }, []);

  // 对话框打开时检查更新
  // 关闭瞬间不重置状态，避免在退出动画阶段闪现“正在检查更新”
  useEffect(() => {
    if (!open) return;
    checkUpdate();
  }, [open, checkUpdate]);

  // 监听下载进度事件
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
    downloadCancelled.current = false;
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
      if (!downloadCancelled.current) {
        logError("Failed to download update", e);
        setErrorMsg(getOperationErrorMessage(e));
        setStatus("error");
      }
    }
  };

  const cancelDownload = () => {
    downloadCancelled.current = true;
    setStatus("update-available");
    void invoke("cancel_update_download").catch((error) => {
      logError("Failed to cancel update download", error);
      setErrorMsg(getOperationErrorMessage(error));
      setStatus("error");
    });
  };

  const installUpdate = async () => {
    if (!installerPath) return;
    setStatus("installing");
    try {
      await invoke("install_update", { installerPath });
    } catch (e) {
      logError("Failed to install update", e);
      setErrorMsg(getOperationErrorMessage(e));
      setStatus("error");
    }
  };

  const progressPercent =
    progress.total > 0
      ? Math.round((progress.downloaded / progress.total) * 100)
      : 0;

  // 下载或安装期间禁止关闭
  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && (status === "downloading" || status === "installing"))
      return;
    onOpenChange(newOpen);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="w-[min(96vw,1100px)] max-w-[1100px] max-h-[90vh] overflow-hidden"
        showCloseButton={status !== "downloading" && status !== "installing"}
      >
        <DialogHeader>
          <DialogTitle>{t("settings.update.title")}</DialogTitle>
          {status === "update-available" && updateInfo && (
            <DialogDescription className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <span>
                v{updateInfo.current_version} → v{updateInfo.latest_version}
              </span>
              {outdatedVersionCount > 0 && (
                <span className="text-status-warning">
                  {t("settings.update.outdated", { count: outdatedVersionCount })}
                </span>
              )}
            </DialogDescription>
          )}
        </DialogHeader>

        {/* Checking */}
        {status === "checking" && (
          <div className="flex items-center justify-center gap-2 py-8">
            <ArrowSync16Regular className="w-5 h-5 text-primary animate-spin" />
            <span className="text-sm text-muted-foreground">
              {t("settings.update.checking")}
            </span>
          </div>
        )}

        {/* No update */}
        {status === "no-update" && (
          <div className="flex flex-col items-center gap-2 py-8">
            <CheckmarkCircle16Regular className="w-8 h-8 text-primary" />
            <span className="text-sm font-medium">{t("settings.update.upToDate")}</span>
            <span className="text-xs text-muted-foreground">
              v{updateInfo?.current_version}
            </span>
          </div>
        )}

        {/* Update available */}
        {status === "update-available" && updateInfo && (
          <>
            {releaseSections.length > 0 && (
              <div className="max-h-[64vh] overflow-y-auto">
                <div className="space-y-2">
                  {releaseSections.map((section) => {
                    const expanded = section.id === expandedSectionId;
                    return (
                      <section
                        key={section.id}
                        className="overflow-hidden rounded-md border bg-background/30"
                      >
                        <button
                          type="button"
                          className="flex w-full items-center justify-between px-3 py-2 text-left hover:bg-accent/30"
                          aria-expanded={expanded}
                          onClick={() =>
                            setExpandedSectionId((prev) =>
                              prev === section.id ? "" : section.id,
                            )
                          }
                        >
                          <span className="text-sm font-semibold text-foreground">
                            v{section.version}
                          </span>
                          <ChevronDown16Regular
                            className={`h-4 w-4 text-muted-foreground transition-transform ${expanded ? "rotate-180" : ""}`}
                          />
                        </button>
                        {expanded && (
                          <div className="border-t px-3 py-2.5">
                            <SimpleMarkdown content={section.content || t("settings.update.noNotes")} />
                          </div>
                        )}
                      </section>
                    );
                  })}
                </div>
              </div>
            )}
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted-foreground">
                {updateInfo.file_size > 0 && formatSize(updateInfo.file_size)}
              </span>
              <Button size="sm" onClick={startDownload}>
                <ArrowDownload16Regular className="w-4 h-4" />
                {t("settings.update.download")}
              </Button>
            </div>
          </>
        )}

        {/* Downloading */}
        {status === "downloading" && (
          <div className="space-y-3 py-4">
            <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-primary rounded-full transition-surface"
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
                {t("settings.update.cancelDownload")}
              </Button>
            </div>
          </div>
        )}

        {/* Downloaded */}
        {status === "downloaded" && (
          <div className="flex flex-col items-center gap-3 py-4">
            <CheckmarkCircle16Regular className="w-8 h-8 text-primary" />
            <span className="text-sm font-medium">{t("settings.update.downloadComplete")}</span>
            <Button onClick={installUpdate}>{t("settings.update.installRestart")}</Button>
          </div>
        )}

        {/* Installing */}
        {status === "installing" && (
          <div className="flex items-center justify-center gap-2 py-8">
            <ArrowSync16Regular className="w-5 h-5 text-primary animate-spin" />
            <span className="text-sm text-muted-foreground">
              {t("settings.update.installing")}
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
              {t("common.retry")}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── 更新日志渲染器 ──

const MAIN_OWNER = "y-aslant";
const MAIN_REPO = "elegantclipboard";
const ISSUE_OR_PR_URL_RE = /^https?:\/\/github\.com\/([^/]+)\/([^/]+)\/(issues|pull)\/(\d+)(?:[/?#].*)?$/i;
const COMMIT_PREFIX_RE =
  /^(fix|feat|chore|docs|refactor|perf|test|style|build|ci|revert)(\([^)]+\))?(!)?:\s*(.+)$/i;
const MD_IMAGE_ONLY_LINE_RE = /^!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)$/i;
const RAW_IMG_TAG_LINE_RE = /^<img\b[^>]*>$/i;
const IMAGE_EXT_RE = /\.(png|jpe?g|gif|webp|bmp|svg|avif)(?:\?.*)?$/i;
const USER_ATTACHMENTS_RE =
  /^https?:\/\/github\.com\/user-attachments\/(assets|files)\/[a-z0-9-]+(?:\?.*)?$/i;
const INLINE_TOKEN_RE =
  /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)|(https?:\/\/[^\s<>"']+)|@([a-zA-Z0-9-]+)/g;
const BOLD_RE = /\*\*(.+?)\*\*/g;

function parseIssueOrPr(url: string): {
  owner: string;
  repo: string;
  number: string;
} | null {
  const match = url.match(ISSUE_OR_PR_URL_RE);
  if (!match) return null;
  return {
    owner: match[1],
    repo: match[2],
    number: match[4],
  };
}

function isImageUrl(url: string): boolean {
  return USER_ATTACHMENTS_RE.test(url) || IMAGE_EXT_RE.test(url);
}

function formatLinkLabel(url: string, explicitLabel?: string, attachmentLabel = "attachment"): string {
  const issue = parseIssueOrPr(url);
  if (issue) {
    const owner = issue.owner.toLowerCase();
    const repo = issue.repo.toLowerCase();
    if (owner === MAIN_OWNER && repo === MAIN_REPO) return `#${issue.number}`;
    return `${issue.owner}/${issue.repo}#${issue.number}`;
  }

  if (explicitLabel && explicitLabel.trim().length > 0 && explicitLabel !== url) {
    return explicitLabel;
  }

  try {
    const u = new URL(url);
    if (u.hostname === "github.com" && isImageUrl(url)) {
      return attachmentLabel;
    }
    const compactPath = u.pathname.replace(/\/+$/, "");
    if (!compactPath) return u.hostname;
    return `${u.hostname}${compactPath}`;
  } catch {
    return explicitLabel || url;
  }
}

function openExternalUrl(url: string) {
  void openUrl(url).catch((error) => {
    reportUserError(t("operationFeedback.userActions.openLink"), error, "Failed to open release note URL");
  });
}

function renderLink(url: string, label: string, key: string): ReactNode {
  return (
    <a
      key={key}
      href={url}
      className="text-primary hover:underline break-all"
      onClick={(e) => {
        e.preventDefault();
        openExternalUrl(url);
      }}
    >
      {label}
    </a>
  );
}

function parseInlineNoBold(text: string, keyPrefix: string, attachmentLabel: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let lastIndex = 0;
  let idx = 0;
  let match: RegExpExecArray | null;
  INLINE_TOKEN_RE.lastIndex = 0;

  while ((match = INLINE_TOKEN_RE.exec(text)) !== null) {
    const start = match.index;
    if (start > lastIndex) {
      nodes.push(
        <span key={`${keyPrefix}-txt-${idx++}`}>{text.slice(lastIndex, start)}</span>,
      );
    }

    if (match[1] && match[2]) {
      const explicit = match[1].trim();
      const url = match[2];
      const prefersCompactLabel = explicit === url || /^https?:\/\//i.test(explicit);
      const label = prefersCompactLabel ? formatLinkLabel(url, undefined, attachmentLabel) : formatLinkLabel(url, explicit, attachmentLabel);
      nodes.push(renderLink(url, label, `${keyPrefix}-lnk-${idx++}`));
    } else if (match[3]) {
      const url = match[3];
      nodes.push(renderLink(url, formatLinkLabel(url, undefined, attachmentLabel), `${keyPrefix}-url-${idx++}`));
    } else if (match[4]) {
      const user = match[4];
      const url = `https://github.com/${user}`;
      nodes.push(renderLink(url, `@${user}`, `${keyPrefix}-usr-${idx++}`));
    }

    lastIndex = INLINE_TOKEN_RE.lastIndex;
  }

  if (lastIndex < text.length) {
    nodes.push(<span key={`${keyPrefix}-tail`}>{text.slice(lastIndex)}</span>);
  }

  return nodes;
}

function parseInline(text: string, keyPrefix: string, attachmentLabel: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let lastIndex = 0;
  let idx = 0;
  let match: RegExpExecArray | null;
  BOLD_RE.lastIndex = 0;

  while ((match = BOLD_RE.exec(text)) !== null) {
    const start = match.index;
    if (start > lastIndex) {
      nodes.push(...parseInlineNoBold(text.slice(lastIndex, start), `${keyPrefix}-p-${idx++}`, attachmentLabel));
    }

    nodes.push(
      <strong key={`${keyPrefix}-b-${idx++}`}>
        {parseInlineNoBold(match[1], `${keyPrefix}-btxt-${idx++}`, attachmentLabel)}
      </strong>,
    );

    lastIndex = BOLD_RE.lastIndex;
  }

  if (lastIndex < text.length) {
    nodes.push(...parseInlineNoBold(text.slice(lastIndex), `${keyPrefix}-tail-${idx++}`, attachmentLabel));
  }

  return nodes;
}

function parseRawImageLine(line: string): { src: string; alt: string } | null {
  const trimmed = line.trim();
  if (!RAW_IMG_TAG_LINE_RE.test(trimmed)) return null;

  const src = trimmed.match(/\bsrc\s*=\s*"([^"]+)"/i)?.[1]
    ?? trimmed.match(/\bsrc\s*=\s*'([^']+)'/i)?.[1];
  if (!src) return null;

  const alt = trimmed.match(/\balt\s*=\s*"([^"]*)"/i)?.[1]
    ?? trimmed.match(/\balt\s*=\s*'([^']*)'/i)?.[1]
    ?? "release-image";

  return { src, alt };
}

function renderReleaseImage(
  src: string,
  alt: string,
  key: string,
): ReactNode {
  return (
    <figure key={key} className="my-2">
      <img
        src={src}
        alt={alt}
        loading="lazy"
        className="block w-full max-h-[240px] object-contain"
      />
    </figure>
  );
}

function getPrefixTagClass(prefix: string): string {
  const key = prefix.toLowerCase();
  if (key === "fix") return "changelog-tag-fix";
  if (key === "feat") return "changelog-tag-feat";
  if (key === "chore") return "changelog-tag-chore";
  if (key === "perf") return "changelog-tag-perf";
  if (key === "refactor") return "changelog-tag-refactor";
  return "changelog-tag-default";
}

function parseCommitPrefix(text: string): {
  label: string;
  detail: string;
  kind: string;
} | null {
  const match = text.match(COMMIT_PREFIX_RE);
  if (!match) return null;
  const kind = match[1].toLowerCase();
  const scope = match[2] ?? "";
  const breaking = match[3] ?? "";
  const detail = match[4] ?? "";
  return {
    label: `${kind}${scope}${breaking}`,
    detail,
    kind,
  };
}

export function SimpleMarkdown({ content }: { content: string }) {
  const { t } = useTranslation();
  const attachmentLabel = t("settings.update.attachmentImage");
  if (!content) return null;

  const lines = content.split(/\r?\n/);
  const nodes: ReactNode[] = [];
  let listItems: ReactNode[] = [];

  const flushList = () => {
    if (listItems.length === 0) return;
    nodes.push(
      <ul key={`ul-${nodes.length}`} className="list-disc pl-4 space-y-1">
        {listItems}
      </ul>,
    );
    listItems = [];
  };

  lines.forEach((rawLine, lineIndex) => {
    const line = rawLine.trim();

    if (!line) {
      flushList();
      nodes.push(<div key={`sp-${lineIndex}`} className="h-1.5" />);
      return;
    }

    const mdImageMatch = line.match(MD_IMAGE_ONLY_LINE_RE);
    if (mdImageMatch && isImageUrl(mdImageMatch[2])) {
      flushList();
      nodes.push(renderReleaseImage(mdImageMatch[2], mdImageMatch[1] || "release-image", `mdimg-${lineIndex}`));
      return;
    }

    const rawImage = parseRawImageLine(line);
    if (rawImage && isImageUrl(rawImage.src)) {
      flushList();
      nodes.push(renderReleaseImage(rawImage.src, rawImage.alt, `rawimg-${lineIndex}`));
      return;
    }

    if (/^##\s+/.test(line)) {
      flushList();
      nodes.push(
        <h3 key={`h2-${lineIndex}`} className="font-semibold text-sm mt-2 text-foreground">
          {parseInline(line.replace(/^##\s+/, ""), `h2-${lineIndex}`, attachmentLabel)}
        </h3>,
      );
      return;
    }

    if (/^###\s+/.test(line)) {
      flushList();
      nodes.push(
        <h4 key={`h3-${lineIndex}`} className="font-medium text-xs mt-1 text-foreground">
          {parseInline(line.replace(/^###\s+/, ""), `h3-${lineIndex}`, attachmentLabel)}
        </h4>,
      );
      return;
    }

    const bullet = line.match(/^[-*]\s+(.+)$/);
    if (bullet) {
      const commit = parseCommitPrefix(bullet[1]);
      listItems.push(
        <li key={`li-${lineIndex}`} className="text-xs text-muted-foreground leading-5">
          {commit ? (
            <div className="flex items-start gap-1.5">
              <span
                className={`inline-flex h-5 shrink-0 items-center self-start rounded-md border px-1.5 text-micro font-semibold uppercase tracking-wide ${getPrefixTagClass(commit.kind)}`}
              >
                {commit.label}
              </span>
              <span className="min-w-0 break-words leading-5">
                {parseInline(commit.detail, `li-${lineIndex}`, attachmentLabel)}
              </span>
            </div>
          ) : (
            parseInline(bullet[1], `li-${lineIndex}`, attachmentLabel)
          )}
        </li>,
      );
      return;
    }

    flushList();
    const asImageUrl = line.match(/^https?:\/\/\S+$/)?.[0];
    if (asImageUrl && isImageUrl(asImageUrl)) {
      nodes.push(renderReleaseImage(asImageUrl, "release-image", `urlimg-${lineIndex}`));
      return;
    }

    nodes.push(
      <p key={`p-${lineIndex}`} className="text-xs text-muted-foreground leading-relaxed break-words">
        {parseInline(line, `p-${lineIndex}`, attachmentLabel)}
      </p>,
    );
  });

  flushList();

  return (
    <div className="space-y-1">{nodes}</div>
  );
}
