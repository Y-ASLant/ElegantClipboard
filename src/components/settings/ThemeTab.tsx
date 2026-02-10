import {
  PaintBrush16Regular,
  Checkmark16Filled,
} from "@fluentui/react-icons";
import { useUISettings, ColorTheme } from "@/stores/ui-settings";
import { useEffect } from "react";

export function ThemeTab() {
  const { colorTheme, setColorTheme } = useUISettings();

  // Apply theme to document
  useEffect(() => {
    // Remove all theme classes
    document.documentElement.classList.remove("theme-emerald", "theme-cyan", "theme-violet");
    // Add current theme class (default doesn't need a class)
    if (colorTheme !== "default") {
      document.documentElement.classList.add(`theme-${colorTheme}`);
    }
  }, [colorTheme]);

  const themes: {
    id: ColorTheme;
    name: string;
    description: string;
    preview: {
      primary: string;
      secondary: string;
    };
  }[] = [
    {
      id: "default",
      name: "默认",
      description: "经典黑白灰配色，简约大气",
      preview: {
        primary: "#1e293b",
        secondary: "#f1f5f9",
      },
    },
    {
      id: "emerald",
      name: "翡翠绿",
      description: "清新自然，护眼舒适",
      preview: {
        primary: "#059669",
        secondary: "#ecfdf5",
      },
    },
    {
      id: "cyan",
      name: "天空青",
      description: "清爽明亮，现代科技",
      preview: {
        primary: "#0891b2",
        secondary: "#ecfeff",
      },
    },
    {
      id: "violet",
      name: "紫罗兰",
      description: "优雅神秘，个性时尚",
      preview: {
        primary: "#7c3aed",
        secondary: "#f5f3ff",
      },
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 pb-2 border-b">
        <PaintBrush16Regular className="w-4 h-4 text-muted-foreground" />
        <h3 className="text-sm font-medium">外观主题</h3>
      </div>

      <div className="space-y-3">
        {themes.map((theme) => (
          <button
            key={theme.id}
            onClick={() => setColorTheme(theme.id)}
            className={`
              w-full flex items-center gap-4 p-4 rounded-lg border-2 transition-all duration-200
              ${colorTheme === theme.id
                ? "border-primary bg-primary/5"
                : "border-border hover:border-primary/50 hover:bg-accent"
              }
            `}
          >
            {/* Color Preview */}
            <div className="flex gap-1.5 shrink-0">
              <div
                className="w-10 h-10 rounded-lg shadow-sm"
                style={{ backgroundColor: theme.preview.primary }}
              />
              <div
                className="w-10 h-10 rounded-lg border shadow-sm"
                style={{ backgroundColor: theme.preview.secondary }}
              />
            </div>

            {/* Theme Info */}
            <div className="flex-1 text-left">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium">{theme.name}</span>
                {colorTheme === theme.id && (
                  <Checkmark16Filled className="w-4 h-4 text-primary" />
                )}
              </div>
              <span className="text-xs text-muted-foreground">
                {theme.description}
              </span>
            </div>
          </button>
        ))}
      </div>

      {/* Preview Section */}
      <div className="rounded-lg border bg-card p-4">
        <div className="text-xs text-muted-foreground mb-3">预览效果</div>
        <div className="space-y-2">
          <button className="w-full py-2 px-4 bg-primary text-primary-foreground rounded-md text-sm font-medium">
            主要按钮
          </button>
          <button className="w-full py-2 px-4 border border-border rounded-md text-sm font-medium hover:bg-accent">
            次要按钮
          </button>
        </div>
      </div>
    </div>
  );
}
