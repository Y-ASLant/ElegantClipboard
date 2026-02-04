import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Settings } from "./pages/Settings";
import "./index.css";

// Disable context menu (right-click)
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

// Disable browser shortcuts
document.addEventListener("keydown", (e) => {
  // Disable F5 refresh
  if (e.key === "F5") {
    e.preventDefault();
  }
  // Disable Ctrl+R refresh
  if (e.ctrlKey && e.key === "r") {
    e.preventDefault();
  }
  // Disable Ctrl+Shift+R hard refresh
  if (e.ctrlKey && e.shiftKey && e.key === "R") {
    e.preventDefault();
  }
  // Disable Ctrl+F5 hard refresh
  if (e.ctrlKey && e.key === "F5") {
    e.preventDefault();
  }
  // Disable Ctrl+F browser search
  if (e.ctrlKey && e.key === "f") {
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
    <Router />
  </React.StrictMode>,
);
