import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import App from "./App";

const Settings = lazy(() => import("./pages/Settings").then(m => ({ default: m.Settings })));
const TextEditor = lazy(() => import("./pages/TextEditor").then(m => ({ default: m.TextEditor })));
const OcrScreenshot = lazy(() => import("./pages/OcrScreenshot").then(m => ({ default: m.OcrScreenshot })));
const OcrResult = lazy(() => import("./pages/OcrResult").then(m => ({ default: m.OcrResult })));
const TranslateResult = lazy(() => import("./pages/TranslateResult").then(m => ({ default: m.TranslateResult })));
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

const ALLOWED_CTRL_LETTER_KEYS = new Set(["a", "c", "v", "x", "z", "y"]);
const BLOCKED_BROWSER_KEYS = new Set(["Tab", "F1", "F3", "F5", "F6", "F7", "F11", "F12"]);

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
  // 拦截 Tab 导航、F1 帮助、F3 查找、F5 刷新、F6 地址栏、F7 光标浏览、F11 全屏、F12 开发者工具
  if (BLOCKED_BROWSER_KEYS.has(e.key)) {
    e.preventDefault();
  }
});

// 基于 URL 路径的简单路由
function Router() {
  const path = window.location.pathname;
  
  if (path === "/settings" || path === "/settings.html") {
    return <Suspense fallback={null}><Settings /></Suspense>;
  }
  if (path === "/editor" || path === "/editor.html") {
    return <Suspense fallback={null}><TextEditor /></Suspense>;
  }
  if (path.startsWith("/ocr-screenshot")) {
    return <Suspense fallback={null}><OcrScreenshot /></Suspense>;
  }
  if (path === "/ocr-result" || path === "/ocr-result.html") {
    return <Suspense fallback={null}><OcrResult /></Suspense>;
  }
  if (path === "/translate-result" || path === "/translate-result.html") {
    return <Suspense fallback={null}><TranslateResult /></Suspense>;
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
