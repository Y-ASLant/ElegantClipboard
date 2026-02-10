import React from "react";
import ReactDOM from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import App from "./App";
import { Settings } from "./pages/Settings";
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

// Disable context menu (right-click)
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

// Disable WebView2 browser shortcuts that leak through to desktop apps
document.addEventListener("keydown", (e) => {
  // Block Ctrl+letter browser shortcuts (Ctrl+R/F/S/P/etc.)
  // Only target single-letter keys so Ctrl+Backspace, Ctrl+Arrow etc. still work
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    const allowed = new Set(["a", "c", "v", "x", "z", "y"]);
    if (!allowed.has(e.key.toLowerCase())) {
      e.preventDefault();
    }
  }
  // Block Tab navigation, F5 refresh, F7 caret browsing
  if (e.key === "Tab" || e.key === "F5" || e.key === "F7") {
    e.preventDefault();
  }
});

// Simple router based on URL path
function Router() {
  const path = window.location.pathname;
  
  if (path === "/settings" || path === "/settings.html") {
    return <Settings />;
  }
  
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={300}>
      <Router />
    </TooltipProvider>
  </React.StrictMode>,
);
