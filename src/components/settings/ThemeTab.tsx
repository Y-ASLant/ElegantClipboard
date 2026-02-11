import { useState, useEffect } from "react";
import {
  Checkmark16Filled,
  Desktop16Regular,
} from "@fluentui/react-icons";
import { getAccentColor, subscribeAccentColor } from "@/lib/theme-applier";
import { useUISettings, ColorTheme } from "@/stores/ui-settings";

export function ThemeTab() {
  const { colorTheme, setColorTheme } = useUISettings();
  const [systemAccentColor, setSystemAccentColor] = useState(getAccentColor);

  // Re-render when accent color changes
  useEffect(() => subscribeAccentColor(setSystemAccentColor), []);

  const themes: {
    id: ColorTheme;
    name: string;
    description: string;
    icon?: React.ComponentType<{ className?: string }>;
    getPreview: () => { primary: string; secondary: string };
  }[] = [
    {
      id: "system",
      name: "跟随系统",
      description: systemAccentColor
        ? "当前系统强调色"
        : "自动适配系统强调色",
      icon: Desktop16Regular,
      getPreview: () => {
        if (!systemAccentColor) return { primary: "#0078d4", secondary: "#f0f0f0" };
        const parts = systemAccentColor.split(" ");
        return {
          primary: `hsl(${parts[0]} ${parts[1] || "65%"} ${parts[2] || "50%"})`,
          secondary: `hsl(${parts[0]} 40% 95%)`,
        };
      },
    },
    {
      id: "default",
      name: "经典黑白",
      description: "经典黑白灰配色，简约大气",
      getPreview: () => ({
        primary: "#1e293b",
        secondary: "#f1f5f9",
      }),
    },
    {
      id: "emerald",
      name: "翡翠绿",
      description: "清新自然，护眼舒适",
      getPreview: () => ({
        primary: "#059669",
        secondary: "#ecfdf5",
      }),
    },
    {
      id: "cyan",
      name: "天空青",
      description: "清爽明亮，现代科技",
      getPreview: () => ({
        primary: "#0891b2",
        secondary: "#ecfeff",
      }),
    },
  ];

  return (
    <div className="space-y-4">
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">外观主题</h3>
        <p className="text-xs text-muted-foreground mb-4">选择应用的配色方案</p>

        <div className="space-y-2">
          {themes.map((theme) => {
            const preview = theme.getPreview();
            const Icon = theme.icon;
            const isActive = colorTheme === theme.id;
            return (
              <button
                key={theme.id}
                onClick={() => setColorTheme(theme.id)}
                className={`
                  w-full flex items-center gap-3 p-3 rounded-md border transition-all duration-200
                  ${isActive
                    ? "border-primary bg-primary/5"
                    : "border-transparent hover:bg-accent"
                  }
                `}
              >
                {/* Color Preview */}
                <div className="flex gap-1.5 shrink-0">
                  <div
                    className="w-8 h-8 rounded-md shadow-sm"
                    style={{ backgroundColor: preview.primary }}
                  />
                  <div
                    className="w-8 h-8 rounded-md border shadow-sm"
                    style={{ backgroundColor: preview.secondary }}
                  />
                </div>

                {/* Theme Info */}
                <div className="flex-1 text-left">
                  <div className="flex items-center gap-2">
                    {Icon && <Icon className="w-3.5 h-3.5 text-muted-foreground" />}
                    <span className="text-xs font-medium">{theme.name}</span>
                    {isActive && (
                      <Checkmark16Filled className="w-3.5 h-3.5 text-primary" />
                    )}
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    {theme.description}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
