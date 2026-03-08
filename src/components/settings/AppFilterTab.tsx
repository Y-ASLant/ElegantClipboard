import { useState, useEffect, useCallback, useRef } from "react";
import { Delete16Regular } from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";

interface AppMeta { name: string; icon: string | null }
type RunningApp = { name: string; process: string; icon: string | null };

const ALL_MONITOR_TYPES = ["text", "html", "rtf", "image", "files"] as const;
const TYPE_LABELS: Record<string, string> = {
  text: "纯文本",
  html: "HTML",
  rtf: "RTF",
  image: "图片",
  files: "文件",
};

export function AppFilterTab() {
  const [appFilterEnabled, setAppFilterEnabled] = useState(false);
  const [appFilterMode, setAppFilterMode] = useState<"blacklist" | "whitelist">("blacklist");
  const [appFilterList, setAppFilterList] = useState<string[]>([]);
  const [excludeInput, setExcludeInput] = useState("");
  const [runningApps, setRunningApps] = useState<RunningApp[]>([]);
  const [showAppPicker, setShowAppPicker] = useState(false);
  const [monitorTypes, setMonitorTypes] = useState<Set<string>>(new Set(ALL_MONITOR_TYPES));
  // 缓存进程名 → 应用信息（名称+图标），从选择器中获取
  const appMetaCache = useRef<Map<string, AppMeta>>(new Map());

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "app_filter_enabled" })
      .then((v) => setAppFilterEnabled(v === "true"))
      .catch(() => {});
    invoke<string | null>("get_setting", { key: "app_filter_mode" })
      .then((v) => { if (v === "whitelist") setAppFilterMode("whitelist"); })
      .catch(() => {});
    invoke<string | null>("get_setting", { key: "app_filter_list" })
      .then((v) => {
        if (v && v.length > 0) {
          setAppFilterList(v.split(",").map((t) => t.trim()).filter(Boolean));
        }
      })
      .catch(() => {});
    invoke<string | null>("get_setting", { key: "monitor_types" })
      .then((v) => {
        if (v && v.length > 0) {
          setMonitorTypes(new Set(v.split(",").map((t) => t.trim()).filter(Boolean)));
        }
      })
      .catch(() => {});
    // 预加载一次运行中应用来填充图标缓存
    invoke<RunningApp[]>("get_running_apps")
      .then((apps) => {
        for (const app of apps) {
          appMetaCache.current.set(app.process.toLowerCase(), { name: app.name, icon: app.icon });
        }
      })
      .catch(() => {});
  }, []);

  const toggleMonitorType = useCallback((type: string) => {
    setMonitorTypes((prev) => {
      const next = new Set(prev);
      if (next.has(type)) {
        // 至少保留一种类型
        if (next.size <= 1) return prev;
        next.delete(type);
      } else {
        next.add(type);
      }
      const value = Array.from(next).join(",");
      invoke("set_setting", { key: "monitor_types", value }).catch(() => {});
      return next;
    });
  }, []);

  const toggleAppFilter = useCallback((enabled: boolean) => {
    setAppFilterEnabled(enabled);
    invoke("set_setting", { key: "app_filter_enabled", value: String(enabled) }).catch(() => {});
  }, []);

  const switchAppFilterMode = useCallback((mode: "blacklist" | "whitelist") => {
    setAppFilterMode(mode);
    invoke("set_setting", { key: "app_filter_mode", value: mode }).catch(() => {});
  }, []);

  const addFilterApp = useCallback((name: string, meta?: AppMeta) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    if (meta) {
      appMetaCache.current.set(trimmed.toLowerCase(), meta);
    }
    setAppFilterList((prev) => {
      if (prev.some((a) => a.toLowerCase() === trimmed.toLowerCase())) return prev;
      const next = [...prev, trimmed];
      invoke("set_setting", { key: "app_filter_list", value: next.join(",") }).catch(() => {});
      return next;
    });
  }, []);

  const removeFilterApp = useCallback((name: string) => {
    setAppFilterList((prev) => {
      const next = prev.filter((a) => a !== name);
      invoke("set_setting", { key: "app_filter_list", value: next.join(",") }).catch(() => {});
      return next;
    });
  }, []);

  const loadRunningApps = useCallback(async () => {
    try {
      const apps = await invoke<RunningApp[]>("get_running_apps");
      setRunningApps(apps);
      for (const app of apps) {
        appMetaCache.current.set(app.process.toLowerCase(), { name: app.name, icon: app.icon });
      }
      setShowAppPicker(true);
    } catch {
      logError("Failed to load running apps");
    }
  }, []);

  const getMeta = (process: string): AppMeta | undefined =>
    appMetaCache.current.get(process.toLowerCase());

  return (
    <div className="space-y-4">
      {/* 监听内容类型 */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">监听内容类型</h3>
        <p className="text-xs text-muted-foreground mb-4">选择要监听的剪贴板内容类型，未勾选的类型将不会被记录</p>
        <div className="flex flex-wrap gap-2">
          {ALL_MONITOR_TYPES.map((type) => {
            const active = monitorTypes.has(type);
            return (
              <button
                key={type}
                type="button"
                onClick={() => toggleMonitorType(type)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-colors ${
                  active
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-muted/40 text-muted-foreground border-transparent hover:bg-muted"
                }`}
              >
                {TYPE_LABELS[type]}
              </button>
            );
          })}
        </div>
      </div>

      {/* 开关 + 模式 */}
      <div className="rounded-lg border bg-card p-4">
        <div className="flex items-center justify-between mb-3">
          <div>
            <h3 className="text-sm font-medium">应用过滤</h3>
            <p className="text-xs text-muted-foreground mt-1">
              根据来源应用决定是否记录剪贴板内容
            </p>
          </div>
          <Switch checked={appFilterEnabled} onCheckedChange={toggleAppFilter} />
        </div>

        <div className="flex gap-1.5">
          {(["blacklist", "whitelist"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              onClick={() => switchAppFilterMode(mode)}
              className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-colors ${
                appFilterMode === mode
                  ? "bg-primary text-primary-foreground border-primary"
                  : "bg-muted/40 text-muted-foreground border-transparent hover:bg-muted"
              }`}
            >
              {mode === "blacklist" ? "黑名单" : "白名单"}
            </button>
          ))}
        </div>

        <p className="text-xs text-muted-foreground mt-3">
          {appFilterMode === "blacklist"
            ? "黑名单模式：来自以下应用的复制内容将不会被记录"
            : "白名单模式：仅记录来自以下应用的复制内容"}
        </p>
      </div>

      {/* 规则列表 */}
      <div className="rounded-lg border bg-card p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-medium">过滤规则</h3>
          <Button variant="outline" size="sm" onClick={loadRunningApps} className="h-7 text-xs">
            选择应用
          </Button>
        </div>

        <div className="flex gap-2 mb-4">
          <Input
            value={excludeInput}
            onChange={(e) => setExcludeInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                addFilterApp(excludeInput);
                setExcludeInput("");
              }
            }}
            placeholder="手动输入进程名或通配符，如 *chrome*"
            className="flex-1 h-8 text-xs"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() => { addFilterApp(excludeInput); setExcludeInput(""); }}
            disabled={!excludeInput.trim()}
            className="h-8 text-xs"
          >
            添加
          </Button>
        </div>

        {appFilterList.length > 0 ? (
          <div className="space-y-1">
            {appFilterList.map((rule) => {
              const meta = getMeta(rule);
              return (
                <div
                  key={rule}
                  className="group flex items-center gap-2.5 px-2.5 py-2 rounded-md bg-muted/40 hover:bg-muted/70 transition-colors"
                >
                  {meta?.icon ? (
                    <img
                      src={convertFileSrc(meta.icon)}
                      alt=""
                      className="w-5 h-5 shrink-0 object-contain"
                      onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                    />
                  ) : (
                    <div className="w-5 h-5 shrink-0 rounded bg-muted flex items-center justify-center text-[10px] text-muted-foreground">
                      {rule.includes("*") || rule.includes("?") ? "*" : "?"}
                    </div>
                  )}
                  <div className="flex-1 min-w-0">
                    {meta ? (
                      <>
                        <div className="text-xs font-medium truncate">{meta.name}</div>
                        <div className="text-[10px] text-muted-foreground truncate">{rule}</div>
                      </>
                    ) : (
                      <div className="text-xs font-medium truncate">{rule}</div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => removeFilterApp(rule)}
                    className="shrink-0 w-6 h-6 rounded-md flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive transition-all"
                    aria-label={`移除 ${rule}`}
                  >
                    <Delete16Regular className="w-3.5 h-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-center py-6">
            <p className="text-xs text-muted-foreground/60">暂无过滤规则</p>
            <p className="text-[10px] text-muted-foreground/40 mt-1">
              点击"选择应用"从运行中的应用添加，或手动输入通配符规则
            </p>
          </div>
        )}
      </div>

      {/* Running Apps Picker Dialog */}
      <Dialog open={showAppPicker} onOpenChange={setShowAppPicker}>
        <DialogContent className="max-w-md max-h-[70vh] flex flex-col">
          <DialogHeader>
            <DialogTitle className="text-sm">选择运行中的应用</DialogTitle>
            <DialogDescription className="text-xs">
              点击应用即可添加到{appFilterMode === "blacklist" ? "黑" : "白"}名单
            </DialogDescription>
          </DialogHeader>
          <div className="flex-1 overflow-y-auto -mx-1 px-1 space-y-0.5">
            {runningApps.map((app) => {
              const alreadyAdded = appFilterList.some(
                (f) => f.toLowerCase() === app.process.toLowerCase()
              );
              return (
                <button
                  key={app.process}
                  type="button"
                  disabled={alreadyAdded}
                  onClick={() => addFilterApp(app.process, { name: app.name, icon: app.icon })}
                  className={`w-full flex items-center gap-2.5 px-2.5 py-2 rounded-md text-left transition-colors ${
                    alreadyAdded
                      ? "opacity-40 cursor-not-allowed"
                      : "hover:bg-accent"
                  }`}
                >
                  {app.icon ? (
                    <img
                      src={convertFileSrc(app.icon)}
                      alt=""
                      className="w-5 h-5 shrink-0 object-contain"
                      onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                    />
                  ) : (
                    <div className="w-5 h-5 shrink-0 rounded bg-muted flex items-center justify-center text-[10px] text-muted-foreground">
                      ?
                    </div>
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-medium truncate">{app.name}</div>
                    <div className="text-[10px] text-muted-foreground truncate">{app.process}</div>
                  </div>
                  {alreadyAdded && (
                    <span className="text-[10px] text-muted-foreground shrink-0">已添加</span>
                  )}
                </button>
              );
            })}
            {runningApps.length === 0 && (
              <p className="text-xs text-muted-foreground text-center py-8">加载中...</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setShowAppPicker(false)} className="text-xs">
              关闭
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
