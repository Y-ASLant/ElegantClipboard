import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logError } from "@/lib/logger";

export type ProxyMode = "system" | "none" | "custom";

const SETTINGS_KEYS = [
  "webdav_enabled", "webdav_auto_sync", "webdav_sync_interval",
  "webdav_url", "webdav_username", "webdav_password", "webdav_remote_dir",
  "webdav_proxy_mode", "webdav_proxy_url", "webdav_accept_invalid_certs",
  "webdav_sync_text", "webdav_sync_image", "webdav_sync_files", "webdav_sync_video",
  "webdav_max_image_size_kb", "webdav_max_file_size_kb", "webdav_max_video_size_kb",
  "webdav_last_sync_time",
] as const;

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
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadSettings = useCallback(async () => {
    try {
      const m = await invoke<Record<string, string>>("get_settings_batch", { keys: SETTINGS_KEYS });

      setEnabled(m["webdav_enabled"] === "true");
      setAutoSync(m["webdav_auto_sync"] === "true");
      setSyncInterval(m["webdav_sync_interval"] || "60");
      setUrl(m["webdav_url"] || "");
      setUsername(m["webdav_username"] || "");
      setPassword(m["webdav_password"] || "");
      setRemoteDir(m["webdav_remote_dir"] || "/elegant-clipboard");
      const pm = m["webdav_proxy_mode"] || "system";
      setProxyMode(pm === "none" || pm === "custom" ? pm : "system");
      setProxyUrl(m["webdav_proxy_url"] || "");
      setAcceptInvalidCerts(m["webdav_accept_invalid_certs"] === "true");

      const types = new Set<string>();
      if (m["webdav_sync_text"] !== "false") types.add("text");
      if (m["webdav_sync_image"] !== "false") types.add("image");
      if (m["webdav_sync_files"] !== "false") types.add("files");
      if (m["webdav_sync_video"] === "true") types.add("video");
      setSyncTypes(types);

      setMaxImageSizeKb(m["webdav_max_image_size_kb"] || "5120");
      setMaxFileSizeKb(m["webdav_max_file_size_kb"] || "5120");
      setMaxVideoSizeKb(m["webdav_max_video_size_kb"] || "5120");
      setLastSyncTime(m["webdav_last_sync_time"] || "");
      setLoaded(true);
    } catch (error) {
      logError("加载同步设置失败:", error);
    }
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

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
    saveSetting("webdav_accept_invalid_certs", acceptInvalidCerts ? "true" : "false");
  }, [acceptInvalidCerts, loaded, saveSetting]);

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