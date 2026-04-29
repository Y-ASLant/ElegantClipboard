import React from "react";
import ReactDOM from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import App from "./App";
import { Settings } from "./pages/Settings";
import { TextEditor } from "./pages/TextEditor";
import { OcrScreenshot } from "./pages/OcrScreenshot";
import { OcrResult } from "./pages/OcrResult";
import { TranslateResult } from "./pages/TranslateResult";
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

const ALLOWED_CTRL_LETTER_KEYS = new Set(["a", "c", "v", "x", "z", "y"]);
const BLOCKED_BROWSER_KEYS = new Set(["Tab", "F5", "F7"]);

// 禁用右键菜单
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

// 禁用 WebView2 浏览器快捷键
document.addEventListener("keydown", (e) => {
  // 拦截 Ctrl+字母浏览器快捷键，保留 Ctrl+Backspace/Arrow 等
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    if (!ALLOWED_CTRL_LETTER_KEYS.has(e.key.toLowerCase())) {
      e.preventDefault();
    }
  }
  // 拦截 Tab 导航、F5 刷新、F7 光标浏览
  if (BLOCKED_BROWSER_KEYS.has(e.key)) {
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
  if (path.startsWith("/ocr-screenshot")) {
    return <OcrScreenshot />;
  }
  if (path === "/ocr-result" || path === "/ocr-result.html") {
    return <OcrResult />;
  }
  if (path === "/translate-result" || path === "/translate-result.html") {
    return <TranslateResult />;
  }
  
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={300} skipDelayDuration={0} disableHoverableContent>
      <Router />
    </TooltipProvider>
  </React.StrictMode>,
);
