import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t } from "@/i18n";
import { logError } from "@/lib/logger";
import { reportUserError } from "@/lib/operation-feedback";
import { notifyWebDAVAvailabilityChanged } from "@/lib/webdav-availability";

export type ProxyMode = "system" | "none" | "custom";

const SETTINGS_KEYS = [
  "webdav_enabled", "webdav_auto_sync", "webdav_sync_interval",
  "webdav_url", "webdav_username", "webdav_password", "webdav_remote_dir",
  "webdav_proxy_mode", "webdav_proxy_url", "webdav_accept_invalid_certs",
  "webdav_sync_text", "webdav_sync_image", "webdav_sync_files", "webdav_sync_video",
  "webdav_max_image_size_kb", "webdav_max_file_size_kb", "webdav_max_video_size_kb",
  "webdav_last_sync_time",
] as const;

const DEBOUNCE_KEYS: Record<string, true> = {
  webdav_url: true, webdav_username: true, webdav_password: true,
  webdav_remote_dir: true, webdav_proxy_url: true,
  webdav_max_image_size_kb: true, webdav_max_file_size_kb: true, webdav_max_video_size_kb: true,
};

export function useWebDAVSettings() {
  const [enabled, setEnabled] = useState(false);
  const [autoSync, setAutoSync] = useState(false);
  const [syncInterval, setSyncInterval] = useState("60");
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [remoteDir, setRemoteDir] = useState("/elegant-clipboard");
  const [proxyMode, setProxyMode] = useState<ProxyMode>("system");
  const [proxyUrl, setProxyUrl] = useState("");
  const [acceptInvalidCerts, setAcceptInvalidCerts] = useState(false);
  const [syncTypes, setSyncTypes] = useState<Set<string>>(new Set(["text", "image", "files"]));
  const [maxImageSizeKb, setMaxImageSizeKb] = useState("5120");
  const [maxFileSizeKb, setMaxFileSizeKb] = useState("5120");
  const [maxVideoSizeKb, setMaxVideoSizeKb] = useState("5120");
  const [lastSyncTime, setLastSyncTime] = useState("");
  const [loaded, setLoaded] = useState(false);
  const saveTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  const snapshotRef = useRef<Record<string, string>>({});
  const currentRef = useRef<Record<string, string>>({});
  const savingRef = useRef(new Map<string, { queued: boolean }>());
  const loadGenerationRef = useRef(0);

  const loadSettings = useCallback(async () => {
    try {
      const m = await invoke<Record<string, string>>("get_settings_batch", { keys: SETTINGS_KEYS });
      const pm = m["webdav_proxy_mode"];
      const normalized: Record<string, string> = {
        webdav_enabled: m["webdav_enabled"] === "true" ? "true" : "false",
        webdav_auto_sync: m["webdav_auto_sync"] === "true" ? "true" : "false",
        webdav_sync_interval: m["webdav_sync_interval"] || "60",
        webdav_url: m["webdav_url"] || "",
        webdav_username: m["webdav_username"] || "",
        webdav_password: m["webdav_password"] || "",
        webdav_remote_dir: m["webdav_remote_dir"] || "/elegant-clipboard",
        webdav_proxy_mode: pm === "none" || pm === "custom" ? pm : "system",
        webdav_proxy_url: m["webdav_proxy_url"] || "",
        webdav_accept_invalid_certs: m["webdav_accept_invalid_certs"] === "true" ? "true" : "false",
        webdav_sync_text: m["webdav_sync_text"] !== "false" ? "true" : "false",
        webdav_sync_image: m["webdav_sync_image"] !== "false" ? "true" : "false",
        webdav_sync_files: m["webdav_sync_files"] !== "false" ? "true" : "false",
        webdav_sync_video: m["webdav_sync_video"] === "true" ? "true" : "false",
        webdav_max_image_size_kb: m["webdav_max_image_size_kb"] || "5120",
        webdav_max_file_size_kb: m["webdav_max_file_size_kb"] || "5120",
        webdav_max_video_size_kb: m["webdav_max_video_size_kb"] || "5120",
      };
      for (const timer of saveTimersRef.current.values()) clearTimeout(timer);
      saveTimersRef.current.clear();
      for (const saving of savingRef.current.values()) saving.queued = false;
      loadGenerationRef.current += 1;
      snapshotRef.current = { ...normalized };
      currentRef.current = normalized;

      setEnabled(normalized.webdav_enabled === "true");
      setAutoSync(normalized.webdav_auto_sync === "true");
      setSyncInterval(normalized.webdav_sync_interval);
      setUrl(normalized.webdav_url);
      setUsername(normalized.webdav_username);
      setPassword(normalized.webdav_password);
      setRemoteDir(normalized.webdav_remote_dir);
      setProxyMode(normalized.webdav_proxy_mode as ProxyMode);
      setProxyUrl(normalized.webdav_proxy_url);
      setAcceptInvalidCerts(normalized.webdav_accept_invalid_certs === "true");
      setSyncTypes(new Set(["text", "image", "files", "video"].filter((type) => normalized[`webdav_sync_${type}`] === "true")));
      setMaxImageSizeKb(normalized.webdav_max_image_size_kb);
      setMaxFileSizeKb(normalized.webdav_max_file_size_kb);
      setMaxVideoSizeKb(normalized.webdav_max_video_size_kb);
      setLastSyncTime(m["webdav_last_sync_time"] || "");
      setLoaded(true);
    } catch (error) {
      logError("加载同步设置失败:", error);
    }
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  const saveSetting = useCallback(async function saveSetting(key: string, value: string): Promise<void> {
    const pending = savingRef.current.get(key);
    if (pending) {
      pending.queued = true;
      return;
    }
    if (snapshotRef.current[key] === value) return;
    const saving = { queued: false };
    savingRef.current.set(key, saving);
    const generation = loadGenerationRef.current;
    try {
      await invoke("set_setting", { key, value });
      if (generation === loadGenerationRef.current || saving.queued) snapshotRef.current[key] = value;
      if (key === "webdav_enabled") notifyWebDAVAvailabilityChanged();
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.saveSettings"), error, `Failed to save ${key}`);
    } finally {
      savingRef.current.delete(key);
      const latest = currentRef.current[key];
      // 同一字段串行保存；失败值留到下一次编辑重试，后续新值不被旧请求吞掉。
      if (latest !== value && !saveTimersRef.current.has(key) &&
          (generation === loadGenerationRef.current || saving.queued)) {
        void saveSetting(key, latest);
      }
    }
  }, []);

  const current = useMemo<Record<string, string>>(() => ({
    webdav_enabled: enabled ? "true" : "false",
    webdav_auto_sync: autoSync ? "true" : "false",
    webdav_sync_interval: syncInterval,
    webdav_url: url,
    webdav_username: username,
    webdav_password: password,
    webdav_remote_dir: remoteDir,
    webdav_proxy_mode: proxyMode,
    webdav_proxy_url: proxyUrl,
    webdav_accept_invalid_certs: acceptInvalidCerts ? "true" : "false",
    webdav_sync_text: syncTypes.has("text") ? "true" : "false",
    webdav_sync_image: syncTypes.has("image") ? "true" : "false",
    webdav_sync_files: syncTypes.has("files") ? "true" : "false",
    webdav_sync_video: syncTypes.has("video") ? "true" : "false",
    webdav_max_image_size_kb: maxImageSizeKb,
    webdav_max_file_size_kb: maxFileSizeKb,
    webdav_max_video_size_kb: maxVideoSizeKb,
  }), [enabled, autoSync, syncInterval, url, username, password, remoteDir, proxyMode, proxyUrl,
    acceptInvalidCerts, syncTypes, maxImageSizeKb, maxFileSizeKb, maxVideoSizeKb]);

  useEffect(() => {
    if (!loaded) return;
    const previous = currentRef.current;
    currentRef.current = current;
    for (const [key, value] of Object.entries(current)) {
      const timer = saveTimersRef.current.get(key);
      if (timer && previous[key] === value) continue;
      if (timer) {
        clearTimeout(timer);
        saveTimersRef.current.delete(key);
      }
      if (snapshotRef.current[key] === value &&
          (!savingRef.current.has(key) || previous[key] === value)) continue;
      if (DEBOUNCE_KEYS[key]) {
        saveTimersRef.current.set(key, setTimeout(() => {
          saveTimersRef.current.delete(key);
          void saveSetting(key, value);
        }, 300));
      } else {
        void saveSetting(key, value);
      }
    }
  }, [loaded, current, saveSetting]);

  return {
    enabled, setEnabled,
    autoSync, setAutoSync,
    syncInterval, setSyncInterval,
    url, setUrl,
    username, setUsername,
    password, setPassword,
    remoteDir, setRemoteDir,
    proxyMode, setProxyMode,
    proxyUrl, setProxyUrl,
    acceptInvalidCerts, setAcceptInvalidCerts,
    syncTypes, setSyncTypes,
    maxImageSizeKb, setMaxImageSizeKb,
    maxFileSizeKb, setMaxFileSizeKb,
    maxVideoSizeKb, setMaxVideoSizeKb,
    lastSyncTime,
    loadSettings,
  };
}
