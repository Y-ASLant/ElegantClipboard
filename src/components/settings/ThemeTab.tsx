import {
  PaintBrush16Regular,
  Checkmark16Filled,
  Desktop16Regular,
} from "@fluentui/react-icons";
import { getAccentColor } from "@/lib/theme-applier";
import { useUISettings, ColorTheme } from "@/stores/ui-settings";

export function ThemeTab() {
  const { colorTheme, setColorTheme } = useUISettings();
  const systemAccentColor = getAccentColor();

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
          primary: `hsl(${parts[0]} ${parts[1] || "65%"} 35%)`,
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
      <div className="flex items-center gap-2 pb-2 border-b">
        <PaintBrush16Regular className="w-4 h-4 text-muted-foreground" />
        <h3 className="text-sm font-medium">外观主题</h3>
      </div>

      <div className="space-y-3">
        {themes.map((theme) => {
          const preview = theme.getPreview();
          const Icon = theme.icon;
          return (
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
                  style={{ backgroundColor: preview.primary }}
                />
                <div
                  className="w-10 h-10 rounded-lg border shadow-sm"
                  style={{ backgroundColor: preview.secondary }}
                />
              </div>

              {/* Theme Info */}
              <div className="flex-1 text-left">
                <div className="flex items-center gap-2">
                  {Icon && <Icon className="w-4 h-4 text-muted-foreground" />}
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
          );
        })}
      </div>
    </div>
  );
}
