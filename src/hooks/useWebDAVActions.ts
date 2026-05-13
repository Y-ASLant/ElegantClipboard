import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useOcrSettings } from "@/stores/ocr-settings";
import { useTranslateSettings } from "@/stores/translate-settings";
import { useTtsSettings } from "@/stores/tts-settings";
import { loadSyncedSettings } from "@/stores/ui-settings";

export type SyncStatusType = "success" | "error" | "info";

export function useWebDAVActions() {
  const [testing, setTesting] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const [statusType, setStatusType] = useState<SyncStatusType>("info");

  const handleTestConnection = useCallback(async () => {
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
  }, []);

  const handleUpload = useCallback(async () => {
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
  }, []);

  const handleDownload = useCallback(async () => {
    setSyncing(true);
    setStatusMsg("");
    try {
      const msg = await invoke<string>("webdav_download");
      setStatusMsg(msg);
      setStatusType("success");
      await invoke("reload_runtime_settings").catch(() => {});
      await loadSyncedSettings();
      await useOcrSettings.getState().loadSettings();
      await useTranslateSettings.getState().loadSettings();
      await useTtsSettings.getState().loadSettings();
      emit("clipboard-updated").catch(() => {});
      emit("tags-updated").catch(() => {});
    } catch (error) {
      setStatusMsg(String(error));
      setStatusType("error");
    } finally {
      setSyncing(false);
    }
  }, []);

  return {
    testing,
    syncing,
    statusMsg,
    statusType,
    setStatusMsg,
    setStatusType,
    handleTestConnection,
    handleUpload,
    handleDownload,
  };
}