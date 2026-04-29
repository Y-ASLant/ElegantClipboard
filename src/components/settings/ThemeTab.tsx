import { useEffect, useMemo, useState } from "react";
import {
  Checkmark16Filled,
  Desktop16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { useShallow } from "zustand/react/shallow";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";
import { getAccentColor, subscribeAccentColor } from "@/lib/theme-applier";
import { useUISettings, ColorTheme, DarkMode, WindowEffect } from "@/stores/ui-settings";

// ============ 系统字体缓存（避免每次切换选项卡重新枚举） ============
let _cachedFonts: string[] | null = null;
let _fontPromise: Promise<string[]> | null = null;

function fetchSystemFontsOnce(): Promise<string[]> {
  if (_cachedFonts) return Promise.resolve(_cachedFonts);
  if (!_fontPromise) {
    _fontPromise = invoke<string[]>("get_system_fonts")
      .then((fonts) => { _cachedFonts = fonts; return fonts; })
      .catch((error) => { logError("Failed to load system fonts:", error); _fontPromise = null; return []; });
  }
  return _fontPromise;
}

// ============ Windows 11 检测缓存 ============
let _isWin11: boolean | null = null;
let _win11Promise: Promise<boolean> | null = null;

function checkIsWindows11Once(): Promise<boolean> {
  if (_isWin11 !== null) return Promise.resolve(_isWin11);
  if (!_win11Promise) {
    _win11Promise = invoke<boolean>("is_windows_11")
      .then((v) => { _isWin11 = v; return v; })
      .catch(() => { _isWin11 = false; _win11Promise = null; return false; });
  }
  return _win11Promise;
}

const DARK_MODE_OPTIONS: { value: DarkMode; label: string }[] = [
  { value: "auto", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

function FontSettingGroup({ label, fonts, font, onFontChange, fontSize, onFontSizeChange, min, max }: {
  label: string;
  fonts: string[];
  font: string;
  onFontChange: (v: string) => void;
  fontSize: number;
  onFontSizeChange: (v: number) => void;
  min: number;
  max: number;
}) {
  return (
    <div className="space-y-3">
      <Label className="text-xs font-medium">{label}</Label>
      <Select value={font || "__default__"} onValueChange={(v) => onFontChange(v === "__default__" ? "" : v)}>
        <SelectTrigger className="w-full h-8 text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent className="max-h-64 overflow-y-auto">
          <SelectItem value="__default__" className="text-xs">默认字体</SelectItem>
          {fonts.map((f) => (
            <SelectItem key={f} value={f} className="text-xs">{f}</SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className="flex items-center justify-between">
        <Label className="text-xs">字号</Label>
        <span className="text-xs font-medium tabular-nums">{fontSize}px</span>
      </div>
      <Slider value={[fontSize]} onValueChange={(v) => onFontSizeChange(v[0])} min={min} max={max} step={1} />
    </div>
  );
}

export function ThemeTab() {
  const {
    colorTheme, setColorTheme, sharpCorners, setSharpCorners, darkMode, setDarkMode,
    windowEffect, setWindowEffect, customFont, setCustomFont, uiFontSize, setUIFontSize,
    cardFont, setCardFont, cardFontSize, setCardFontSize,
    previewFont, setPreviewFont, previewFontSize, setPreviewFontSize,
    resetFontSettings,
  } = useUISettings(useShallow((s) => ({
    colorTheme: s.colorTheme, setColorTheme: s.setColorTheme,
    sharpCorners: s.sharpCorners, setSharpCorners: s.setSharpCorners,
    darkMode: s.darkMode, setDarkMode: s.setDarkMode,
    windowEffect: s.windowEffect, setWindowEffect: s.setWindowEffect,
    customFont: s.customFont, setCustomFont: s.setCustomFont,
    uiFontSize: s.uiFontSize, setUIFontSize: s.setUIFontSize,
    cardFont: s.cardFont, setCardFont: s.setCardFont,
    cardFontSize: s.cardFontSize, setCardFontSize: s.setCardFontSize,
    previewFont: s.previewFont, setPreviewFont: s.setPreviewFont,
    previewFontSize: s.previewFontSize, setPreviewFontSize: s.setPreviewFontSize,
    resetFontSettings: s.resetFontSettings,
  })));
  const [systemAccentColor, setSystemAccentColor] = useState(getAccentColor);
  const [systemFonts, setSystemFonts] = useState<string[]>(() => _cachedFonts ?? []);
  const [isWin11, setIsWin11] = useState<boolean>(() => _isWin11 ?? true);

  // 强调色变化时重新渲染
  useEffect(() => subscribeAccentColor(setSystemAccentColor), []);

  // 加载系统字体列表（首次枚举后缓存，后续从 useState 初始化直接命中）
  useEffect(() => {
    if (_cachedFonts) return;
    fetchSystemFontsOnce().then(setSystemFonts);
  }, []);

  // 检测是否为 Windows 11
  useEffect(() => {
    if (_isWin11 !== null) return;
    checkIsWindows11Once().then(setIsWin11);
  }, []);

  const themes = useMemo<{
    id: ColorTheme;
    name: string;
    description: string;
    icon?: React.ComponentType<{ className?: string }>;
    getPreview: () => { primary: string; secondary: string };
  }[]>(() => [
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
  ], [systemAccentColor]);

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
                  className={`relative z-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition-colors ${
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
          ]).map((opt) => {
            const needsWin11 = opt.value !== "none";
            const disabled = needsWin11 && !isWin11;
            return (
              <button
                key={opt.value}
                onClick={() => !disabled && setWindowEffect(opt.value)}
                disabled={disabled}
                className={`flex flex-col items-start p-3 rounded-md border transition-all duration-200 text-left ${
                  disabled
                    ? "opacity-50 cursor-not-allowed border-transparent"
                    : windowEffect === opt.value
                      ? "border-primary bg-primary/5"
                      : "border-transparent hover:bg-accent"
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium">{opt.label}</span>
                  {!disabled && windowEffect === opt.value && (
                    <Checkmark16Filled className="w-3.5 h-3.5 text-primary" />
                  )}
                </div>
                <span className="text-[11px] text-muted-foreground mt-0.5">
                  {disabled ? "仅支持 Windows 11" : opt.desc}
                </span>
              </button>
            );
          })}
        </div>
      </div>
      {/* Font Settings */}
      <div className="rounded-lg border bg-card p-4 space-y-5">
        <div className="flex items-start justify-between">
          <div>
            <h3 className="text-sm font-medium mb-1">字体设置</h3>
            <p className="text-xs text-muted-foreground">分别设置界面、卡片内容和悬浮预览的字体</p>
          </div>
          <button
            type="button"
            className="text-xs text-muted-foreground hover:text-foreground transition-colors shrink-0"
            onClick={resetFontSettings}
          >
            重置
          </button>
        </div>

        <FontSettingGroup label="界面字体" fonts={systemFonts} font={customFont} onFontChange={setCustomFont} fontSize={uiFontSize} onFontSizeChange={setUIFontSize} min={12} max={18} />
        <hr className="border-border" />
        <FontSettingGroup label="卡片内容字体" fonts={systemFonts} font={cardFont} onFontChange={setCardFont} fontSize={cardFontSize} onFontSizeChange={setCardFontSize} min={12} max={18} />
        <hr className="border-border" />
        <FontSettingGroup label="悬浮预览字体" fonts={systemFonts} font={previewFont} onFontChange={setPreviewFont} fontSize={previewFontSize} onFontSizeChange={setPreviewFontSize} min={11} max={18} />
      </div>
    </div>
  );
}
