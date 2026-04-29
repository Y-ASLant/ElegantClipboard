import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";
import { loadSyncedSettings } from "@/stores/ui-settings";
import { useOcrSettings } from "@/stores/ocr-settings";
import { useTranslateSettings } from "@/stores/translate-settings";
import {
  ArrowSync16Regular,
  ArrowUp16Regular,
  ArrowDown16Regular,
  Checkmark16Regular,
  Eye16Regular,
  EyeOff16Regular,
} from "@fluentui/react-icons";

export function SyncTab() {
  // WebDAV 配置
  const [enabled, setEnabled] = useState(false);
  const [autoSync, setAutoSync] = useState(false);
  const [syncInterval, setSyncInterval] = useState("60");
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [remoteDir, setRemoteDir] = useState("/elegant-clipboard");
  const [proxyMode, setProxyMode] = useState<"system" | "none" | "custom">("system");
  const [proxyUrl, setProxyUrl] = useState("");

  // 同步选项
  const [syncTypes, setSyncTypes] = useState<Set<string>>(new Set(["text", "image", "files"]));
  const [maxImageSizeKb, setMaxImageSizeKb] = useState("5120");
  const [maxFileSizeKb, setMaxFileSizeKb] = useState("5120");
  const [maxVideoSizeKb, setMaxVideoSizeKb] = useState("5120");

  // 状态
  const [testing, setTesting] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const [statusType, setStatusType] = useState<"success" | "error" | "info">("info");
  const [lastSyncTime, setLastSyncTime] = useState("");
  const [loaded, setLoaded] = useState(false);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const statusMsgRef = useRef<HTMLDivElement>(null);

  // 加载设置
  useEffect(() => {
    loadSettings();
  }, []);

  // 状态消息出现时自动滚动到可见区域
  useEffect(() => {
    if (statusMsg && statusMsgRef.current) {
      statusMsgRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }, [statusMsg]);

  // 监听后台媒体同步完成事件
  useEffect(() => {
    const unlisten = listen<string>("media-sync-done", (event) => {
      setStatusMsg((prev) => prev ? `${prev}\n${event.payload}` : event.payload);
      setStatusType("success");
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const loadSettings = async () => {
    try {
      const keys = [
        "webdav_enabled", "webdav_auto_sync", "webdav_sync_interval",
        "webdav_url", "webdav_username", "webdav_password", "webdav_remote_dir",
        "webdav_proxy_mode", "webdav_proxy_url",
        "webdav_sync_text", "webdav_sync_image", "webdav_sync_files", "webdav_sync_video",
        "webdav_max_image_size_kb", "webdav_max_file_size_kb", "webdav_max_video_size_kb",
        "webdav_last_sync_time",
      ];
      const values = await Promise.all(
        keys.map((key) => invoke<string | null>("get_setting", { key }))
      );
      const m = new Map(keys.map((k, i) => [k, values[i]]));

      setEnabled(m.get("webdav_enabled") === "true");
      setAutoSync(m.get("webdav_auto_sync") === "true");
      setSyncInterval(m.get("webdav_sync_interval") || "60");
      setUrl(m.get("webdav_url") || "");
      setUsername(m.get("webdav_username") || "");
      setPassword(m.get("webdav_password") || "");
      setRemoteDir(m.get("webdav_remote_dir") || "/elegant-clipboard");
      const pm = m.get("webdav_proxy_mode") || "system";
      setProxyMode(pm === "none" || pm === "custom" ? pm : "system");
      setProxyUrl(m.get("webdav_proxy_url") || "");
      const types = new Set<string>();
      if (m.get("webdav_sync_text") !== "false") types.add("text");
      if (m.get("webdav_sync_image") !== "false") types.add("image");
      if (m.get("webdav_sync_files") !== "false") types.add("files");
      if (m.get("webdav_sync_video") === "true") types.add("video");
      setSyncTypes(types);
      setMaxImageSizeKb(m.get("webdav_max_image_size_kb") || "5120");
      setMaxFileSizeKb(m.get("webdav_max_file_size_kb") || "5120");
      setMaxVideoSizeKb(m.get("webdav_max_video_size_kb") || "5120");
      setLastSyncTime(m.get("webdav_last_sync_time") || "");
      setLoaded(true);
    } catch (error) {
      logError("加载同步设置失败:", error);
    }
  };

  // 自动保存（debounced）
  const saveSetting = useCallback(async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (error) {
      logError(`保存 ${key} 失败:`, error);
    }
  }, []);

  const debouncedSave = useCallback((key: string, value: string) => {
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => saveSetting(key, value), 300);
  }, [saveSetting]);

  // 自动保存各设置
  useEffect(() => {
    if (!loaded) return;
    saveSetting("webdav_enabled", enabled ? "true" : "false");
  }, [enabled, loaded, saveSetting]);

  useEffect(() => {
    if (!loaded) return;
    saveSetting("webdav_auto_sync", autoSync ? "true" : "false");
  }, [autoSync, loaded, saveSetting]);

  useEffect(() => {
    if (!loaded) return;
    saveSetting("webdav_sync_interval", syncInterval);
  }, [syncInterval, loaded, saveSetting]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_url", url);
  }, [url, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_username", username);
  }, [username, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_password", password);
  }, [password, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_remote_dir", remoteDir);
  }, [remoteDir, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    saveSetting("webdav_proxy_mode", proxyMode);
  }, [proxyMode, loaded, saveSetting]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_proxy_url", proxyUrl);
  }, [proxyUrl, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    saveSetting("webdav_sync_text", syncTypes.has("text") ? "true" : "false");
    saveSetting("webdav_sync_image", syncTypes.has("image") ? "true" : "false");
    saveSetting("webdav_sync_files", syncTypes.has("files") ? "true" : "false");
    saveSetting("webdav_sync_video", syncTypes.has("video") ? "true" : "false");
  }, [syncTypes, loaded, saveSetting]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_max_image_size_kb", maxImageSizeKb);
  }, [maxImageSizeKb, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_max_file_size_kb", maxFileSizeKb);
  }, [maxFileSizeKb, loaded, debouncedSave]);

  useEffect(() => {
    if (!loaded) return;
    debouncedSave("webdav_max_video_size_kb", maxVideoSizeKb);
  }, [maxVideoSizeKb, loaded, debouncedSave]);

  // 测试连接
  const handleTestConnection = async () => {
    setTesting(true);
    setStatusMsg("");
    try {
      const msg = await invoke<string>("webdav_test_connection");
      setStatusMsg(msg);
      setStatusType("success");
    } catch (error) {
      setStatusMsg(String(error));
      setStatusType("error");
    } finally {
      setTesting(false);
    }
  };

  // 上传
  const handleUpload = async () => {
    setSyncing(true);
    setStatusMsg("");
    try {
      const msg = await invoke<string>("webdav_upload");
      setStatusMsg(msg);
      setStatusType("success");
    } catch (error) {
      setStatusMsg(String(error));
      setStatusType("error");
    } finally {
      setSyncing(false);
    }
  };

  // 下载
  const handleDownload = async () => {
    setSyncing(true);
    setStatusMsg("");
    try {
      const msg = await invoke<string>("webdav_download");
      setStatusMsg(msg);
      setStatusType("success");
      // 后端重新加载运行时设置（快捷键、托盘图标、游戏模式等）
      await invoke("reload_runtime_settings").catch(() => {});
      // 前端刷新从云端同步的设置
      await loadSyncedSettings();
      await useOcrSettings.getState().loadSettings();
      await useTranslateSettings.getState().loadSettings();
      // 通知主窗口刷新列表
      emit("clipboard-updated").catch(() => {});
    } catch (error) {
      setStatusMsg(String(error));
      setStatusType("error");
    } finally {
      setSyncing(false);
    }
  };

  const imageSizeMB = Math.round(parseInt(maxImageSizeKb || "0") / 1024);
  const fileSizeMB = Math.round(parseInt(maxFileSizeKb || "0") / 1024);
  const videoSizeMB = Math.round(parseInt(maxVideoSizeKb || "0") / 1024);

  return (
    <>
      {/* WebDAV 连接 */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">WebDAV 同步</h3>
        <p className="text-xs text-muted-foreground mb-4">
          通过 WebDAV 在多台设备间同步剪贴板数据
        </p>
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">启用同步</Label>
              <p className="text-xs text-muted-foreground">
                开启后将允许与 WebDAV 服务器同步数据
              </p>
            </div>
            <Switch checked={enabled} onCheckedChange={setEnabled} />
          </div>

          {enabled && (
            <>
              <div className="space-y-2 pt-1">
                <div className="space-y-1.5">
                  <Label className="text-xs">WebDAV 地址</Label>
                  <Input
                    className="h-8 text-xs"
                    placeholder="https://dav.example.com"
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                  />
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <div className="space-y-1.5">
                    <Label className="text-xs">用户名</Label>
                    <Input
                      className="h-8 text-xs"
                      placeholder="username"
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">密码</Label>
                    <div className="relative">
                      <Input
                        className="h-8 text-xs pr-8"
                        type={showPassword ? "text" : "password"}
                        placeholder="password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                      />
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                        onClick={() => setShowPassword(!showPassword)}
                      >
                        {showPassword ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                </div>
                <div className="space-y-1.5">
                  <Label className="text-xs">远端目录</Label>
                  <Input
                    className="h-8 text-xs"
                    placeholder="/elegant-clipboard"
                    value={remoteDir}
                    onChange={(e) => setRemoteDir(e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label className="text-xs">网络代理</Label>
                  <div className="flex items-center gap-2">
                    <Select value={proxyMode} onValueChange={(v) => setProxyMode(v as "system" | "none" | "custom")}>
                      <SelectTrigger className="w-[130px] h-8 text-xs shrink-0"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="system">系统代理</SelectItem>
                        <SelectItem value="none">不使用代理</SelectItem>
                        <SelectItem value="custom">自定义代理</SelectItem>
                      </SelectContent>
                    </Select>
                    {proxyMode === "custom" && (
                      <Input
                        className="h-8 text-xs flex-1"
                        placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                        value={proxyUrl}
                        onChange={(e) => setProxyUrl(e.target.value)}
                      />
                    )}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-2 pt-1">
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs"
                  onClick={handleTestConnection}
                  disabled={testing || !url}
                >
                  {testing ? (
                    <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
                  ) : (
                    <Checkmark16Regular className="w-3.5 h-3.5 mr-1" />
                  )}
                  测试连接
                </Button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* 同步选项 */}
      {enabled && (
        <>
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">同步内容类型</h3>
            <p className="text-xs text-muted-foreground mb-4">
              选择要同步的剪贴板记录类型，软件设置始终同步
            </p>
            <div className="space-y-3">
              <div className="flex flex-wrap gap-2">
                {(["text", "image", "files", "video"] as const).map((type) => {
                  const active = syncTypes.has(type);
                  const label = { text: "文本", image: "图片", files: "文件", video: "视频" }[type];
                  return (
                    <button
                      key={type}
                      type="button"
                      onClick={() => {
                        setSyncTypes((prev) => {
                          const next = new Set(prev);
                          if (next.has(type)) {
                            next.delete(type);
                            if (next.size === 0) return prev;
                          } else {
                            next.add(type);
                          }
                          return next;
                        });
                      }}
                      className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-colors ${
                        active
                          ? "bg-primary text-primary-foreground border-primary"
                          : "bg-muted/40 text-muted-foreground border-transparent hover:bg-muted"
                      }`}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
              {syncTypes.has("image") && (
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label className="text-xs">图片大小限制</Label>
                    <p className="text-xs text-muted-foreground">
                      仅同步 {imageSizeMB > 0 ? `${imageSizeMB} MB` : "不限"} 以内的图片
                    </p>
                  </div>
                  <Select value={maxImageSizeKb} onValueChange={setMaxImageSizeKb}>
                    <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="1024">1 MB</SelectItem>
                      <SelectItem value="2048">2 MB</SelectItem>
                      <SelectItem value="5120">5 MB</SelectItem>
                      <SelectItem value="10240">10 MB</SelectItem>
                      <SelectItem value="20480">20 MB</SelectItem>
                      <SelectItem value="51200">50 MB</SelectItem>
                      <SelectItem value="0">不限</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              )}
              {syncTypes.has("files") && (
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label className="text-xs">文件大小限制</Label>
                    <p className="text-xs text-muted-foreground">
                      仅同步 {fileSizeMB > 0 ? `${fileSizeMB} MB` : "不限"} 以内的文件
                    </p>
                  </div>
                  <Select value={maxFileSizeKb} onValueChange={setMaxFileSizeKb}>
                    <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="1024">1 MB</SelectItem>
                      <SelectItem value="2048">2 MB</SelectItem>
                      <SelectItem value="5120">5 MB</SelectItem>
                      <SelectItem value="10240">10 MB</SelectItem>
                      <SelectItem value="20480">20 MB</SelectItem>
                      <SelectItem value="51200">50 MB</SelectItem>
                      <SelectItem value="102400">100 MB</SelectItem>
                      <SelectItem value="0">不限</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              )}
              {syncTypes.has("video") && (
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label className="text-xs">视频大小限制</Label>
                    <p className="text-xs text-muted-foreground">
                      仅同步 {videoSizeMB > 0 ? `${videoSizeMB} MB` : "不限"} 以内的视频
                    </p>
                  </div>
                  <Select value={maxVideoSizeKb} onValueChange={setMaxVideoSizeKb}>
                    <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="1024">1 MB</SelectItem>
                      <SelectItem value="2048">2 MB</SelectItem>
                      <SelectItem value="5120">5 MB</SelectItem>
                      <SelectItem value="10240">10 MB</SelectItem>
                      <SelectItem value="20480">20 MB</SelectItem>
                      <SelectItem value="51200">50 MB</SelectItem>
                      <SelectItem value="102400">100 MB</SelectItem>
                      <SelectItem value="0">不限</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>
          </div>

          {/* 自动同步 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">自动同步</h3>
            <p className="text-xs text-muted-foreground mb-4">
              定时自动同步，保持多设备数据一致
            </p>
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">自动同步</Label>
                  <p className="text-xs text-muted-foreground">
                    启用后按照设定间隔自动执行同步
                  </p>
                </div>
                <Switch checked={autoSync} onCheckedChange={setAutoSync} />
              </div>
              {autoSync && (
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label className="text-xs">同步间隔</Label>
                    <p className="text-xs text-muted-foreground">
                      每隔多长时间自动同步一次
                    </p>
                  </div>
                  <Select
                    value={syncInterval}
                    onValueChange={setSyncInterval}
                  >
                    <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="30">30 秒</SelectItem>
                      <SelectItem value="60">1 分钟</SelectItem>
                      <SelectItem value="180">3 分钟</SelectItem>
                      <SelectItem value="300">5 分钟</SelectItem>
                      <SelectItem value="600">10 分钟</SelectItem>
                      <SelectItem value="1200">20 分钟</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>
          </div>

          {/* 手动同步 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">手动同步</h3>
            <p className="text-xs text-muted-foreground mb-4">
              立即执行同步操作（覆盖远端文件，避免数据膨胀）
            </p>
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  className="h-7 text-xs"
                  onClick={handleUpload}
                  disabled={syncing || !url}
                >
                  {syncing ? (
                    <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
                  ) : (
                    <ArrowUp16Regular className="w-3.5 h-3.5 mr-1" />
                  )}
                  上传至云端
                </Button>
                <Button
                  size="sm"
                  className="h-7 text-xs"
                  onClick={handleDownload}
                  disabled={syncing || !url}
                >
                  {syncing ? (
                    <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
                  ) : (
                    <ArrowDown16Regular className="w-3.5 h-3.5 mr-1" />
                  )}
                  下载至本地
                </Button>
              </div>

              {lastSyncTime && (
                <p className="text-xs text-muted-foreground">
                  上次同步：{lastSyncTime}
                </p>
              )}

              {statusMsg && (
                <div
                  ref={statusMsgRef}
                  className={`text-xs px-3 py-2 rounded-md whitespace-pre-line ${
                    statusType === "success"
                      ? "bg-green-500/10 text-green-600 dark:text-green-400"
                      : statusType === "error"
                        ? "bg-destructive/10 text-destructive"
                        : "bg-muted text-muted-foreground"
                  }`}
                >
                  {statusMsg}
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </>
  );
}
