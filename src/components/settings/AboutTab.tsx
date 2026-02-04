import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import {
  Person16Regular,
  Code16Regular,
  Open16Regular,
} from "@fluentui/react-icons";

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
    <div className="space-y-4">
      {/* App Info Card */}
      <div className="rounded-lg border bg-card p-6">
        <div className="flex flex-col items-center text-center space-y-4">
          <div className="h-16 w-16 rounded-2xl overflow-hidden">
            <img src="/icon.png" alt="ElegantClipboard" className="w-full h-full object-contain" />
          </div>
          <div className="space-y-2">
            <h3 className="font-semibold text-lg">ElegantClipboard</h3>
            <span className="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-blue-100 text-blue-600 dark:bg-blue-900/30 dark:text-blue-400">
              v{appVersion}
            </span>
          </div>
          <p className="text-sm text-muted-foreground max-w-xs">
            高性能 Windows 剪贴板管理器，支持文本、图片、HTML、RTF、文件路径
          </p>
        </div>
      </div>

      {/* Author Info Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">作者信息</h3>
        <div className="space-y-2">
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Person16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">作者</span>
            </div>
            <span className="text-sm font-medium">ASLant</span>
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
              <Open16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">项目地址</span>
            </div>
            <button
              onClick={() => openUrl("https://github.com/Y-ASLant/ElegantClipboard")}
              className="text-sm font-medium text-primary hover:underline"
            >
              ElegantClipboard
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
