import { useState, useEffect } from "react";
import {
  Person16Regular,
  Code16Regular,
  Star16Regular,
  Alert16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";

export function AboutTab() {
  const [appVersion, setAppVersion] = useState("0.0.0");

  useEffect(() => {
    invoke<string>("get_app_version").then(setAppVersion).catch(console.error);
  }, []);

  const openUrl = async (url: string) => {
    try {
      await tauriOpenUrl(url);
    } catch (error) {
      console.error("Failed to open URL:", error);
    }
  };

  return (
    <>
      {/* App Info Card */}
      <div className="flex-1 rounded-lg border bg-card p-6 flex flex-col justify-center overflow-auto">
        <div className="flex flex-col items-center text-center space-y-4">
          <div className="h-16 w-16 rounded-2xl overflow-hidden">
            <img
              src="/icon.png"
              alt="ElegantClipboard"
              className="w-full h-full object-contain"
            />
          </div>
          <div className="space-y-2">
            <h3 className="font-semibold text-lg">ElegantClipboard</h3>
            <button
              onClick={() =>
                openUrl("https://github.com/Y-ASLant/ElegantClipboard/releases")
              }
              className="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-primary/10 text-primary cursor-pointer hover:shadow-sm hover:scale-105 active:scale-95 transition-all duration-200"
            >
              v{appVersion}
            </button>
          </div>
          <p className="text-sm text-muted-foreground max-w-xs">
            低占用 · 高性能 · 现代化 · 完全本地化离线剪贴板
          </p>
        </div>
      </div>

      {/* Author Info Card */}
      <div className="flex-1 rounded-lg border bg-card p-4 flex flex-col overflow-auto">
        <h3 className="text-sm font-medium mb-3 text-primary">作者信息</h3>
        <div className="space-y-2 flex-1 flex flex-col justify-center">
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Person16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">作者</span>
            </div>
            <span className="text-sm font-medium text-primary">ASLant</span>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Code16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">GitHub</span>
            </div>
            <button
              onClick={() => openUrl("https://github.com/Y-ASLant")}
              className="text-sm font-medium text-primary hover:underline"
            >
              @Y-ASLant
            </button>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Star16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">给个Star</span>
            </div>
            <button
              onClick={() =>
                openUrl("https://github.com/Y-ASLant/ElegantClipboard")
              }
              className="text-sm font-medium text-primary hover:underline"
            >
              ElegantClipboard
            </button>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Alert16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">反馈问题</span>
            </div>
            <button
              onClick={() =>
                openUrl(
                  "https://github.com/Y-ASLant/ElegantClipboard/issues",
                )
              }
              className="text-sm font-medium text-primary hover:underline"
            >
              提交Issue
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
