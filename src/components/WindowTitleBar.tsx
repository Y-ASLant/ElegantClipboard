import { Pin16Filled, Pin16Regular } from "@fluentui/react-icons";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Card } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { reportUserError } from "@/lib/operation-feedback";

// Chromium's app-region style is supported by Tauri but absent from csstype.
const noDragStyle: React.CSSProperties & { WebkitAppRegion: "no-drag" } = { WebkitAppRegion: "no-drag" };

interface WindowTitleBarProps {
  icon: React.ReactNode;
  title: string;
  /** 标题后的额外内容（如未保存指示器） */
  extra?: React.ReactNode;
  /** 居中区域内容 */
  center?: React.ReactNode;
  /** 固定按钮（替代最小化/关闭，用于翻译窗口等） */
  pinControl?: {
    pinned: boolean;
    onToggle: () => void;
    pinLabel: string;
    unpinLabel: string;
  };
}

export function WindowTitleBar({ icon, title, extra, center, pinControl }: WindowTitleBarProps) {
  const { t } = useTranslation();
  return (
    <Card className="shrink-0">
      <div
        className="relative h-11 flex items-center justify-between px-4 select-none"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-3">
          {icon}
          <span className="text-sm font-semibold">{title}</span>
          {extra}
        </div>
        {center && (
          <div className="absolute left-1/2 -translate-x-1/2" style={noDragStyle}>
            {center}
          </div>
        )}
        <div
          className="flex gap-1"
          style={noDragStyle}
        >
          {pinControl ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={pinControl.onToggle}
                  className={`interactive-surface w-8 h-8 flex items-center justify-center rounded-md transition-surface ${
                    pinControl.pinned
                      ? "text-primary bg-primary-subtle"
                      : "text-muted-foreground hover:bg-accent"
                  }`}
                >
                  {pinControl.pinned ? (
                    <Pin16Filled className="w-4 h-4" />
                  ) : (
                    <Pin16Regular className="w-4 h-4" />
                  )}
                </button>
              </TooltipTrigger>
              <TooltipContent>
                {pinControl.pinned ? pinControl.unpinLabel : pinControl.pinLabel}
              </TooltipContent>
            </Tooltip>
          ) : (
            <>
              <button
                aria-label={t("operationFeedback.userActions.minimizeWindow")}
                onClick={async () => {
                  try {
                    await getCurrentWindow().minimize();
                  } catch (error) {
                    reportUserError(t("operationFeedback.userActions.minimizeWindow"), error);
                  }
                }}
                className="interactive-surface w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-surface"
              >
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                  <rect
                    x="2"
                    y="5.5"
                    width="8"
                    height="1"
                    rx="0.5"
                    fill="currentColor"
                  />
                </svg>
              </button>
              <button
                aria-label={t("operationFeedback.userActions.closeWindow")}
                onClick={async () => {
                  try {
                    await getCurrentWindow().close();
                  } catch (error) {
                    reportUserError(t("operationFeedback.userActions.closeWindow"), error);
                  }
                }}
                className="interactive-surface w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground rounded-md transition-surface"
              >
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                  <path
                    d="M2.5 2.5L9.5 9.5M9.5 2.5L2.5 9.5"
                    stroke="currentColor"
                    strokeWidth="1.2"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </>
          )}
        </div>
      </div>
    </Card>
  );
}
