import { getCurrentWindow } from "@tauri-apps/api/window";
import { Card } from "@/components/ui/card";

interface WindowTitleBarProps {
  icon: React.ReactNode;
  title: string;
  /** Extra content shown after the title (e.g. unsaved indicator) */
  extra?: React.ReactNode;
}

export function WindowTitleBar({ icon, title, extra }: WindowTitleBarProps) {
  return (
    <Card className="shrink-0">
      <div
        className="h-11 flex items-center justify-between px-4 select-none"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-3">
          {icon}
          <span className="text-sm font-semibold">{title}</span>
          {extra}
        </div>
        <div
          className="flex gap-1"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <button
            onClick={() => getCurrentWindow().minimize()}
            className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-colors"
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
            onClick={() => getCurrentWindow().close()}
            className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground rounded-md transition-colors"
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
        </div>
      </div>
    </Card>
  );
}
