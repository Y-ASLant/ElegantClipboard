import { useEffect, useState } from "react";
import {
  Checkmark16Filled,
  Desktop16Regular,
} from "@fluentui/react-icons";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { getAccentColor, subscribeAccentColor } from "@/lib/theme-applier";
import { useUISettings, ColorTheme, DarkMode, WindowEffect } from "@/stores/ui-settings";

const DARK_MODE_OPTIONS: { value: DarkMode; label: string }[] = [
  { value: "auto", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

export function ThemeTab() {
  const { colorTheme, setColorTheme, sharpCorners, setSharpCorners, darkMode, setDarkMode, windowEffect, setWindowEffect } = useUISettings();
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

  const activeDarkModeIndex = Math.max(
    0,
    DARK_MODE_OPTIONS.findIndex((opt) => opt.value === darkMode),
  );

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

      {/* Dark Mode */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">深色模式</h3>
        <p className="text-xs text-muted-foreground mb-4">控制应用的明暗外观</p>
        <div
          role="radiogroup"
          aria-label="深色模式"
          className="relative rounded-lg border bg-muted/40 p-1"
        >
          <div className="relative grid grid-cols-3">
            <div
              aria-hidden
              className="absolute inset-y-0 left-0 w-1/3 rounded-md bg-primary shadow-sm will-change-transform transition-transform duration-200 ease-out"
              style={{ transform: `translateX(${activeDarkModeIndex * 100}%)` }}
            />
            {DARK_MODE_OPTIONS.map((opt) => {
              const isActive = darkMode === opt.value;
              return (
                <button
                  key={opt.value}
                  type="button"
                  role="radio"
                  aria-checked={isActive}
                  onClick={() => setDarkMode(opt.value)}
                  className={`relative z-[1] rounded-md px-2.5 py-1.5 text-xs font-medium transition-colors ${
                    isActive
                      ? "text-primary-foreground"
                      : "text-foreground/80 hover:text-foreground"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Sharp Corners */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">圆角</h3>
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">直角模式</Label>
            <p className="text-xs text-muted-foreground">
              使用直角样式，类似 Windows 10 风格
            </p>
          </div>
          <Switch
            checked={sharpCorners}
            onCheckedChange={setSharpCorners}
          />
        </div>
      </div>

      {/* Window Effect */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">窗口特效</h3>
        <p className="text-xs text-muted-foreground mb-4">
          毛玻璃背景效果（需要 Windows 11）
        </p>
        <div className="grid grid-cols-2 gap-2">
          {([
            { value: "none" as WindowEffect, label: "无", desc: "默认不透明背景" },
            { value: "mica" as WindowEffect, label: "Mica", desc: "柔和半透明材质" },
            { value: "acrylic" as WindowEffect, label: "Acrylic", desc: "模糊透明毛玻璃" },
            { value: "tabbed" as WindowEffect, label: "Tabbed", desc: "Mica 变体，更深色调" },
          ]).map((opt) => (
            <button
              key={opt.value}
              onClick={() => setWindowEffect(opt.value)}
              className={`flex flex-col items-start p-3 rounded-md border transition-all duration-200 text-left ${
                windowEffect === opt.value
                  ? "border-primary bg-primary/5"
                  : "border-transparent hover:bg-accent"
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium">{opt.label}</span>
                {windowEffect === opt.value && (
                  <Checkmark16Filled className="w-3.5 h-3.5 text-primary" />
                )}
              </div>
              <span className="text-[11px] text-muted-foreground mt-0.5">
                {opt.desc}
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
