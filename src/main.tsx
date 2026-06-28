import React from "react";
import ReactDOM from "react-dom/client";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initLocale } from "@/i18n";
import { initUISettingsStore } from "@/stores/ui-settings";
import App from "./App";
import { Settings } from "./pages/Settings";
import { TextEditor } from "./pages/TextEditor";
import { TranslateResult } from "./pages/TranslateResult";
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

// 禁用右键菜单
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

// 禁用 WebView2 浏览器快捷键
document.addEventListener("keydown", (e) => {
  // 拦截 Ctrl+字母浏览器快捷键，保留 Ctrl+Backspace/Arrow 等
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    const allowed = new Set(["a", "c", "v", "x", "z", "y"]);
    if (!allowed.has(e.key.toLowerCase())) {
      e.preventDefault();
    }
  }
  // 拦截 Tab 导航、F5 刷新、F7 光标浏览
  if (e.key === "Tab" || e.key === "F1" || e.key === "F3" || e.key === "F5" || e.key === "F6" || e.key === "F7" || e.key === "F11" || e.key === "F12") {
    e.preventDefault();
  }
});

// 基于 URL 路径的简单路由
function Router() {
  const path = window.location.pathname;
  
  if (path === "/settings" || path === "/settings.html") {
    return <Settings />;
  }
  if (path === "/editor" || path === "/editor.html") {
    return <TextEditor />;
  }
  if (path === "/translate-result") {
    return <TranslateResult />;
  }
  
  return <App />;
}

async function bootstrap() {
  try {
    await initLocale();
    await initUISettingsStore();
  } catch (error) {
    console.error("Bootstrap init failed:", error);
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <TooltipProvider delayDuration={300}>
        <Router />
        <Toaster />
      </TooltipProvider>
    </React.StrictMode>,
  );
}

void bootstrap();
