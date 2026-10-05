import { useState, useEffect, useCallback, useMemo } from "react";
import {
  ChevronDown16Regular,
  ChevronUp16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
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
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { getOperationErrorMessage, reportUserError } from "@/lib/operation-feedback";
import { KEY_CODE_MAP } from "@/lib/shortcut-helpers";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";

export interface ShortcutSettings {
  shortcut: string;
  winv_replacement: boolean;
}

type ShortcutEditTarget =
  | { type: "main" }
  | { type: "quick-paste"; slot: number }
  | { type: "favorite-paste"; slot: number };

const QUICK_PASTE_SLOT_COUNT = 10;

type PasteKey = "ctrl_v" | "shift_insert";

const PASTE_KEY_OPTIONS: { value: PasteKey; labelKey: string; descKey: string }[] = [
  { value: "ctrl_v", labelKey: "settings.shortcuts.pasteKeyCtrlV", descKey: "settings.shortcuts.pasteKeyCtrlVDesc" },
  { value: "shift_insert", labelKey: "settings.shortcuts.pasteKeyShiftInsert", descKey: "settings.shortcuts.pasteKeyShiftInsertDesc" },
];

interface ShortcutsTabProps {
  settings: ShortcutSettings;
  onSettingsChange: (settings: ShortcutSettings) => void;
}

export function ShortcutsTab({
  settings,
  onSettingsChange,
}: ShortcutsTabProps) {
  const { t } = useTranslation();
  const keyboardNavigation = useUISettings((s) => s.keyboardNavigation);
  const setKeyboardNavigation = useUISettings((s) => s.setKeyboardNavigation);
  const [winvLoading, setWinvLoading] = useState(false);
  const [winvError, setWinvError] = useState("");
  const [winvConfirmDialogOpen, setWinvConfirmDialogOpen] = useState(false);
  const [winvPendingAction, setWinvPendingAction] = useState<
    "enable" | "disable" | null
  >(null);

  // 快捷键编辑状态
  const [shortcutDialogOpen, setShortcutDialogOpen] = useState(false);
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [tempShortcut, setTempShortcut] = useState("");
  const [shortcutError, setShortcutError] = useState("");
  const [editTarget, setEditTarget] = useState<ShortcutEditTarget | null>(null);
  const [quickPasteShortcuts, setQuickPasteShortcuts] = useState<string[]>([]);
  const [quickPasteLoaded, setQuickPasteLoaded] = useState(false);
  const [loadingSlot, setLoadingSlot] = useState<number | null>(null);
  const [slotErrors, setSlotErrors] = useState<Record<number, string>>({});
  const [quickPasteExpanded, setQuickPasteExpanded] = useState(false);
  const [favPasteShortcuts, setFavPasteShortcuts] = useState<string[]>([]);
  const [favPasteLoaded, setFavPasteLoaded] = useState(false);
  const [favPasteExpanded, setFavPasteExpanded] = useState(false);
  const [favSlotErrors, setFavSlotErrors] = useState<Record<number, string>>({});
  const [pasteKey, setPasteKey] = useState<PasteKey>("ctrl_v");

  const pasteKeyOptions = useMemo(
    () => PASTE_KEY_OPTIONS.map((opt) => ({
      value: opt.value,
      label: t(opt.labelKey),
      desc: t(opt.descKey),
    })),
    [t],
  );
  const activePasteKeyIndex = Math.max(
    0,
    pasteKeyOptions.findIndex((opt) => opt.value === pasteKey),
  );

  useEffect(() => {
    invoke<string | null>("get_setting", { key: "paste_key" })
      .then((val) => {
        if (val === "shift_insert") setPasteKey("shift_insert");
        else setPasteKey("ctrl_v");
      })
      .catch((error) => {
        logError("Failed to load paste key setting:", error);
      });
  }, []);

  const handlePasteKeyChange = async (value: PasteKey) => {
    setPasteKey(value);
    try {
      await invoke("set_setting", { key: "paste_key", value });
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.saveSettings"), error, "Failed to save paste key setting");
    }
  };

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const parts: string[] = [];

    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");

    let key = "";
    if (e.code.startsWith("Key")) {
      key = e.code.replace("Key", "");
    } else if (e.code.startsWith("Digit")) {
      key = e.code.replace("Digit", "");
    } else if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) {
      key = e.code;
    } else {
      key = KEY_CODE_MAP[e.code] || "";
    }

    if (key && parts.length > 0) {
      const hasNonShiftModifier = e.ctrlKey || e.altKey || e.metaKey;
      if (!hasNonShiftModifier) {
        setShortcutError(t("settings.shortcuts.shiftAloneError"));
        return;
      }
      if (e.metaKey && (editTarget?.type === "quick-paste" || editTarget?.type === "favorite-paste")) {
        setShortcutError(t("settings.shortcuts.winNotAllowed"));
        return;
      }
      parts.push(key);
      setTempShortcut(parts.join("+"));
      setShortcutError("");
    } else if (!key && parts.length > 0) {
      setTempShortcut(parts.join("+") + "+...");
    } else if (key && parts.length === 0) {
      setShortcutError(t("settings.shortcuts.needModifier"));
    }
  }, [editTarget, t]);

  // 开始/停止录入
  useEffect(() => {
    if (recordingShortcut) {
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [recordingShortcut, handleKeyDown]);

  useEffect(() => {
    let disposed = false;
    const loadQuickPasteShortcuts = async () => {
      try {
        const shortcuts = await invoke<string[]>("get_quick_paste_shortcuts");
        if (disposed) return;
        if (Array.isArray(shortcuts) && shortcuts.length === QUICK_PASTE_SLOT_COUNT) {
          setQuickPasteShortcuts(shortcuts);
        }
      } catch (error) {
        logError("Failed to load quick paste shortcuts:", error);
      } finally {
        if (!disposed) setQuickPasteLoaded(true);
      }
      try {
        const favShortcuts = await invoke<string[]>("get_favorite_paste_shortcuts");
        if (disposed) return;
        if (Array.isArray(favShortcuts) && favShortcuts.length === QUICK_PASTE_SLOT_COUNT) {
          setFavPasteShortcuts(favShortcuts);
        }
      } catch (error) {
        logError("Failed to load favorite paste shortcuts:", error);
      } finally {
        if (!disposed) setFavPasteLoaded(true);
      }
    };
    loadQuickPasteShortcuts();
    return () => {
      disposed = true;
    };
  }, []);


  const startRecording = () => {
    setRecordingShortcut(true);
    setTempShortcut("");
    setShortcutError("");
  };

  const openEditDialog = (target: ShortcutEditTarget, initialValue: string) => {
    setEditTarget(target);
    setShortcutDialogOpen(true);
    setRecordingShortcut(false);
    setTempShortcut(initialValue);
    setShortcutError("");
  };

  const cancelRecording = () => {
    setRecordingShortcut(false);
    setTempShortcut("");
    setShortcutError("");
    setShortcutDialogOpen(false);
    setEditTarget(null);
  };

  // 标准化快捷键字符串用于比较（顺序无关）
  const normalizeForCompare = (s: string) => s.toLowerCase().split("+").sort().join("+");

  // 当前生效的主快捷键（Win+V 替换开启时为 Win+V）
  const activeMainShortcut = settings.winv_replacement ? "Win+V" : settings.shortcut;

  const detectConflict = useCallback((shortcut: string, target: ShortcutEditTarget): string | null => {
    const normalized = normalizeForCompare(shortcut);
    if (target.type === "quick-paste" && normalized === normalizeForCompare(activeMainShortcut)) {
      return t("settings.shortcuts.conflictMain", { shortcut: activeMainShortcut });
    }
    for (let i = 0; i < quickPasteShortcuts.length; i++) {
      const s = quickPasteShortcuts[i];
      if (!s) continue;
      if (target.type === "quick-paste" && target.slot === i) continue;
      if (normalized === normalizeForCompare(s)) {
        return t("settings.shortcuts.conflictQuick", { num: i + 1 });
      }
    }
    for (let i = 0; i < favPasteShortcuts.length; i++) {
      const s = favPasteShortcuts[i];
      if (!s) continue;
      if (target.type === "favorite-paste" && target.slot === i) continue;
      if (normalized === normalizeForCompare(s)) {
        return t("settings.shortcuts.conflictFavorite", { num: i + 1 });
      }
    }
    return null;
  }, [activeMainShortcut, quickPasteShortcuts, favPasteShortcuts, t]);

  // 通用槽位操作工厂，消除 quick/favorite 重复逻辑
  const createSlotOps = (
    cmd: string,
    setShortcuts: React.Dispatch<React.SetStateAction<string[]>>,
    setErrors: React.Dispatch<React.SetStateAction<Record<number, string>>>,
    slotOffset: number,
  ) => {
    const apply = async (idx: number, shortcut: string) => {
      setLoadingSlot(idx + slotOffset);
      setErrors((prev) => { const { [idx]: _, ...rest } = prev; return rest; });
      await invoke(cmd, { slot: idx + 1, shortcut });
      setShortcuts((prev) => { const next = [...prev]; next[idx] = shortcut; return next; });
    };
    const disable = (idx: number) => {
      apply(idx, "").catch((error) => {
        logError("Failed to disable paste shortcut", error);
        setErrors((prev) => ({ ...prev, [idx]: getOperationErrorMessage(error) }));
      }).finally(() => setLoadingSlot(null));
    };
    const batchReset = async (defaults: string[], currentShortcuts: string[]) => {
      const mainNorm = normalizeForCompare(activeMainShortcut);
      for (let i = 0; i < QUICK_PASTE_SLOT_COUNT; i++) {
        if (currentShortcuts[i] === defaults[i]) continue;
        if (defaults[i] && normalizeForCompare(defaults[i]) === mainNorm) {
          setErrors((prev) => ({ ...prev, [i]: t("settings.shortcuts.skippedConflict", { shortcut: defaults[i] }) }));
          continue;
        }
        try { await apply(i, defaults[i]); } catch (error) {
          logError("Failed to reset paste shortcut", error);
          setErrors((prev) => ({ ...prev, [i]: getOperationErrorMessage(error) }));
        }
      }
      setLoadingSlot(null);
    };
    const batchDisable = async (currentShortcuts: string[]) => {
      for (let i = 0; i < QUICK_PASTE_SLOT_COUNT; i++) {
        if (!currentShortcuts[i]) continue;
        try { await apply(i, ""); } catch (error) {
          logError("Failed to disable paste shortcut", error);
          setErrors((prev) => ({ ...prev, [i]: getOperationErrorMessage(error) }));
        }
      }
      setLoadingSlot(null);
    };
    return { apply, disable, batchReset, batchDisable };
  };

  const quickOps = createSlotOps("set_quick_paste_shortcut", setQuickPasteShortcuts, setSlotErrors, 0);
  const favOps = createSlotOps("set_favorite_paste_shortcut", setFavPasteShortcuts, setFavSlotErrors, 100);

  const saveShortcut = async () => {
    if (!editTarget) {
      setShortcutError(t("settings.shortcuts.noTarget"));
      return;
    }

    if (!tempShortcut || tempShortcut.includes("...")) {
      setShortcutError(t("settings.shortcuts.incomplete"));
      return;
    }

    const conflict = detectConflict(tempShortcut, editTarget);
    if (conflict) {
      setShortcutError(conflict);
      return;
    }

    try {
      if (editTarget.type === "main") {
        await invoke("update_shortcut", { newShortcut: tempShortcut });
        await invoke("set_setting", {
          key: "global_shortcut",
          value: tempShortcut,
        });
        onSettingsChange({ ...settings, shortcut: tempShortcut });
      } else if (editTarget.type === "quick-paste") {
        await quickOps.apply(editTarget.slot, tempShortcut);
      } else {
        await favOps.apply(editTarget.slot, tempShortcut);
      }
      setShortcutDialogOpen(false);
      setRecordingShortcut(false);
      setTempShortcut("");
      setEditTarget(null);
    } catch (error) {
      logError("Failed to save shortcut", error);
      setShortcutError(t("settings.shortcuts.saveFailed", { error: getOperationErrorMessage(error) }));
      if (editTarget.type === "quick-paste") {
        setSlotErrors((prev) => ({ ...prev, [editTarget.slot]: getOperationErrorMessage(error) }));
      } else if (editTarget.type === "favorite-paste") {
        setFavSlotErrors((prev) => ({ ...prev, [editTarget.slot]: getOperationErrorMessage(error) }));
      }
    } finally {
      setLoadingSlot(null);
    }
  };

  const QUICK_DEFAULTS = Array.from({ length: QUICK_PASTE_SLOT_COUNT }, (_, i) => `Alt+${i === 9 ? 0 : i + 1}`);
  const FAV_DEFAULTS = ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "", "", "", "", "", "", ""];

  // 用户确认后执行 Win+V 切换
  const executeWinvToggle = async () => {
    if (!winvPendingAction) return;

    setWinvConfirmDialogOpen(false);
    setWinvLoading(true);
    setWinvError("");

    try {
      if (winvPendingAction === "enable") {
        await invoke("enable_winv_replacement");
        onSettingsChange({ ...settings, winv_replacement: true });
      } else {
        await invoke("disable_winv_replacement");
        onSettingsChange({ ...settings, winv_replacement: false });
      }
    } catch (error) {
      logError("Failed to toggle Win+V replacement:", error);
      setWinvError(getOperationErrorMessage(error));
    } finally {
      setWinvLoading(false);
      setWinvPendingAction(null);
    }
  };

  return (
    <>
      <div className="space-y-3">
        {/* Keyboard Navigation Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.shortcuts.navTitle")}
            description={t("settings.shortcuts.navDesc")}
          />
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.shortcuts.keyboardNav")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.shortcuts.keyboardNavDesc")}
                </p>
              </div>
              <Switch
                checked={keyboardNavigation}
                onCheckedChange={setKeyboardNavigation}
              />
            </div>
          </div>
        </SettingsCard>

        {/* Paste Key Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.shortcuts.pasteKeyTitle")}
            description={t("settings.shortcuts.pasteKeyDesc")}
          />
          <div
            role="radiogroup"
            aria-label={t("settings.shortcuts.pasteKeyAria")}
            className="relative rounded-md border p-1 bg-muted-surface-subtle"
          >
            <div className="relative grid grid-cols-2">
              <div
                aria-hidden
                className="absolute inset-y-0 left-0 w-1/2 rounded-md elevation-control will-change-transform transition-transform duration-200 ease-out bg-primary"
                style={{ transform: `translateX(${activePasteKeyIndex * 100}%)` }}
              />
              {pasteKeyOptions.map((opt) => {
                const isActive = pasteKey === opt.value;
                return (
                  <button
                    key={opt.value}
                    type="button"
                    role="radio"
                    aria-checked={isActive}
                    onClick={() => { void handlePasteKeyChange(opt.value); }}
                    className={cn(
                      "relative z-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition-surface font-mono",
                      isActive
                        ? "text-primary-foreground"
                        : "text-foreground/80 hover:text-foreground",
                    )}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          </div>
          <p className="text-xs text-muted-foreground mt-2">
            {pasteKeyOptions.find((o) => o.value === pasteKey)?.desc}
          </p>
        </SettingsCard>

        {/* Shortcut Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.shortcuts.summonTitle")}
            description={t("settings.shortcuts.summonDesc")}
          />
          <div
            className={cn(
              "space-y-2",
              settings.winv_replacement && "opacity-50",
            )}
          >
            <Label className="text-xs">{t("settings.shortcuts.customShortcut")}</Label>
            <div className="flex gap-2">
              <Input
                value={settings.shortcut}
                readOnly
                className="flex-1 h-8 text-sm font-mono bg-muted"
              />
              <Button
                variant="outline"
                size="sm"
                className="h-8"
                onClick={() => openEditDialog({ type: "main" }, settings.shortcut)}
                disabled={settings.winv_replacement}
              >
                {t("settings.shortcuts.modify")}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              {settings.winv_replacement
                ? t("settings.shortcuts.winvEnabled")
                : t("settings.shortcuts.clickToModify")}
            </p>
          </div>

          <div className="border-t my-4" />

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.shortcuts.useWinV")}</Label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.shortcuts.useWinVDesc")}
                </p>
              </div>
              <Switch
                checked={settings.winv_replacement}
                disabled={winvLoading}
                onCheckedChange={(checked) => {
                  setWinvPendingAction(checked ? "enable" : "disable");
                  setWinvConfirmDialogOpen(true);
                }}
              />
            </div>
            {winvLoading && (
              <p className="text-xs text-muted-foreground">
                {t("settings.shortcuts.modifyingSystem")}
              </p>
            )}
            {winvError && (
              <p className="text-xs text-destructive">{winvError}</p>
            )}
            <p className="text-xs text-status-warning">
              {t("settings.shortcuts.registryWarning")}
            </p>
          </div>
        </SettingsCard>

        {/* Quick Paste Card */}
        <SettingsCard>
          <button
            type="button"
            className="w-full text-left"
            onClick={() => setQuickPasteExpanded((v) => !v)}
          >
            <SettingsCardHeader
              className="mb-0"
              title={t("settings.shortcuts.quickPasteTitle")}
              description={t("settings.shortcuts.quickPasteDesc")}
              action={
                quickPasteExpanded
                  ? <ChevronUp16Regular className="w-4 h-4 text-muted-foreground shrink-0" />
                  : <ChevronDown16Regular className="w-4 h-4 text-muted-foreground shrink-0" />
              }
            />
          </button>

          <div
            className={cn(
              "grid transition-[grid-template-rows] duration-200 ease-in-out",
              quickPasteExpanded ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
            )}
          >
            <div className="overflow-hidden">
              <div className="pt-4 space-y-2">
                {/* Batch operations */}
                <div className="flex gap-2 mb-3">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    disabled={loadingSlot !== null}
                    onClick={() => quickOps.batchReset(QUICK_DEFAULTS, quickPasteShortcuts)}
                  >
                    {t("settings.shortcuts.resetAllDefault")}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs text-muted-foreground"
                    disabled={loadingSlot !== null || quickPasteShortcuts.every((s) => !s)}
                    onClick={() => quickOps.batchDisable(quickPasteShortcuts)}
                  >
                    {t("settings.shortcuts.disableAll")}
                  </Button>
                </div>

                {(quickPasteLoaded ? quickPasteShortcuts : Array.from({ length: QUICK_PASTE_SLOT_COUNT }, (_, i) => `Alt+${i === 9 ? 0 : i + 1}`)).map((shortcut, idx) => (
                  <div key={idx}>
                    <div className="flex items-center gap-2">
                      <Label className="text-xs w-28 shrink-0">{t("settings.shortcuts.quickPasteSlot", { num: idx === 9 ? 10 : idx + 1 })}</Label>
                      <Input
                        value={shortcut}
                        placeholder={t("settings.shortcuts.clickToSet")}
                        readOnly
                        className={cn(
                          "h-8 text-sm flex-1 bg-muted",
                          shortcut && "font-mono",
                          slotErrors[idx] && "border-destructive",
                        )}
                      />
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-8"
                        disabled={loadingSlot === idx}
                        onClick={() => openEditDialog({ type: "quick-paste", slot: idx }, shortcut)}
                      >
                        {t("settings.shortcuts.modify")}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 text-muted-foreground"
                        disabled={loadingSlot === idx || !shortcut}
                        onClick={() => quickOps.disable(idx)}
                      >
                        {t("common.disable")}
                      </Button>
                    </div>
                    {slotErrors[idx] && (
                      <p className="text-xs text-destructive mt-1 ml-28 pl-2">{slotErrors[idx]}</p>
                    )}
                  </div>
                ))}
              </div>
              <p className="text-xs text-muted-foreground mt-3">
                {t("settings.shortcuts.quickPasteHint")}
              </p>
            </div>
          </div>
        </SettingsCard>

        {/* Favorite Paste Card */}
        <SettingsCard>
          <button
            type="button"
            className="w-full text-left"
            onClick={() => setFavPasteExpanded((v) => !v)}
          >
            <SettingsCardHeader
              className="mb-0"
              title={t("settings.shortcuts.favoritePasteTitle")}
              description={t("settings.shortcuts.favoritePasteDesc")}
              action={
                favPasteExpanded
                  ? <ChevronUp16Regular className="w-4 h-4 text-muted-foreground shrink-0" />
                  : <ChevronDown16Regular className="w-4 h-4 text-muted-foreground shrink-0" />
              }
            />
          </button>

          <div
            className={cn(
              "grid transition-[grid-template-rows] duration-200 ease-in-out",
              favPasteExpanded ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
            )}
          >
            <div className="overflow-hidden">
              <div className="pt-4 space-y-2">
                {/* Batch operations */}
                <div className="flex gap-2 mb-3">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    disabled={loadingSlot !== null}
                    onClick={() => favOps.batchReset(FAV_DEFAULTS, favPasteShortcuts)}
                  >
                    {t("settings.shortcuts.resetAllDefault")}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs text-muted-foreground"
                    disabled={loadingSlot !== null || favPasteShortcuts.every((s) => !s)}
                    onClick={() => favOps.batchDisable(favPasteShortcuts)}
                  >
                    {t("settings.shortcuts.disableAll")}
                  </Button>
                </div>

                {(favPasteLoaded ? favPasteShortcuts : ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "", "", "", "", "", "", ""]).map((shortcut, idx) => (
                  <div key={idx}>
                    <div className="flex items-center gap-2">
                      <Label className="text-xs w-28 shrink-0">{t("settings.shortcuts.favoritePasteSlot", { num: idx === 9 ? 10 : idx + 1 })}</Label>
                      <Input
                        value={shortcut}
                        placeholder={t("settings.shortcuts.clickToSet")}
                        readOnly
                        className={cn(
                          "h-8 text-sm flex-1 bg-muted",
                          shortcut && "font-mono",
                          favSlotErrors[idx] && "border-destructive",
                        )}
                      />
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-8"
                        disabled={loadingSlot === idx + 100}
                        onClick={() => openEditDialog({ type: "favorite-paste", slot: idx }, shortcut)}
                      >
                        {t("settings.shortcuts.modify")}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 text-muted-foreground"
                        disabled={loadingSlot === idx + 100 || !shortcut}
                        onClick={() => favOps.disable(idx)}
                      >
                        {t("common.disable")}
                      </Button>
                    </div>
                    {favSlotErrors[idx] && (
                      <p className="text-xs text-destructive mt-1 ml-28 pl-2">{favSlotErrors[idx]}</p>
                    )}
                  </div>
                ))}
              </div>
              <p className="text-xs text-muted-foreground mt-3">
                {t("settings.shortcuts.favoritePasteHint")}
              </p>
            </div>
          </div>
        </SettingsCard>

        {/* Current Active Card */}
        <SettingsCard>
          <SettingsCardHeader
            title={t("settings.shortcuts.activeTitle")}
            description={
              settings.winv_replacement
                ? t("settings.shortcuts.activeWinV")
                : t("settings.shortcuts.activeCustom", { shortcut: settings.shortcut })
            }
          />
          <div className="space-y-2">
            <div className="flex items-center justify-between py-2 px-3 rounded-md bg-primary-subtle border border-primary-subtle">
              <span className="text-sm font-medium">{t("settings.shortcuts.toggleWindow")}</span>
              <kbd className="pointer-events-none inline-flex h-6 select-none items-center gap-1 rounded-md border bg-background px-2 font-mono text-xs font-medium">
                {settings.winv_replacement ? "Win+V" : settings.shortcut}
              </kbd>
            </div>
            {quickPasteLoaded && quickPasteShortcuts.some((s) => s) && (
              <div className="space-y-1">
                {quickPasteShortcuts.map((shortcut, idx) =>
                  shortcut ? (
                    <div key={idx} className="flex items-center justify-between py-1.5 px-3 rounded-md bg-muted-surface">
                      <span className="text-xs text-muted-foreground">{t("settings.shortcuts.quickPasteLabel", { num: idx + 1 })}</span>
                      <kbd className="pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded-md border bg-background px-1.5 font-mono text-micro font-medium">
                        {shortcut}
                      </kbd>
                    </div>
                  ) : null,
                )}
              </div>
            )}
            {favPasteLoaded && favPasteShortcuts.some((s) => s) && (
              <div className="space-y-1 mt-1">
                {favPasteShortcuts.map((shortcut, idx) =>
                  shortcut ? (
                    <div key={`fav-${idx}`} className="flex items-center justify-between py-1.5 px-3 rounded-md bg-muted-surface">
                      <span className="text-xs text-muted-foreground">{t("settings.shortcuts.favoritePasteLabel", { num: idx + 1 })}</span>
                      <kbd className="pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded-md border bg-background px-1.5 font-mono text-micro font-medium">
                        {shortcut}
                      </kbd>
                    </div>
                  ) : null,
                )}
              </div>
            )}
          </div>
          <p className="text-xs text-muted-foreground mt-2">
            {t("settings.shortcuts.mutualExclusive")}
          </p>
        </SettingsCard>
      </div>

      {/* Shortcut Edit Dialog */}
      <Dialog
        open={shortcutDialogOpen}
        onOpenChange={(open) => {
          if (!open) cancelRecording();
          else setShortcutDialogOpen(open);
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {editTarget?.type === "quick-paste"
                ? t("settings.shortcuts.editQuickPaste", { num: editTarget.slot + 1 })
                : editTarget?.type === "favorite-paste"
                  ? t("settings.shortcuts.editFavoritePaste", { num: editTarget.slot + 1 })
                  : t("settings.shortcuts.editShortcut")}
            </DialogTitle>
            <DialogDescription>
              {editTarget?.type === "quick-paste" || editTarget?.type === "favorite-paste"
                ? t("settings.shortcuts.editQuickPasteHint")
                : t("settings.shortcuts.editSummonHint")}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div
              className={cn(
                "h-16 flex items-center justify-center rounded-md border-2 border-dashed transition-surface",
                recordingShortcut
                  ? "border-primary bg-primary-faint"
                  : "border-muted",
              )}
              onClick={startRecording}
            >
              {recordingShortcut ? (
                <span className={cn("text-lg font-medium", tempShortcut && "font-mono")}>
                  {tempShortcut || t("settings.shortcuts.pressShortcut")}
                </span>
              ) : (
                <span className="text-sm text-muted-foreground">
                  {t("settings.shortcuts.clickToRecord")}
                </span>
              )}
            </div>

            {shortcutError && (
              <p className="text-sm text-destructive">{shortcutError}</p>
            )}

            {tempShortcut.includes("Shift") && /\d/.test(tempShortcut) && (
              <p className="text-xs text-status-warning">
                {t("settings.shortcuts.numpadWarning")}
              </p>
            )}

            <p className="text-xs text-muted-foreground">
              {t("settings.shortcuts.modifierHint")}
            </p>
          </div>

          <DialogFooter className="flex justify-between sm:justify-between">
            <Button
              variant="ghost"
              onClick={() => {
                if (editTarget?.type === "quick-paste") {
                  setTempShortcut(`Alt+${editTarget.slot + 1}`);
                } else if (editTarget?.type === "favorite-paste") {
                  const favDefaults = ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3"];
                  setTempShortcut(favDefaults[editTarget.slot] || "");
                } else {
                  setTempShortcut("Alt+C");
                }
                setRecordingShortcut(false);
                setShortcutError("");
              }}
              className="text-muted-foreground"
            >
              {t("settings.shortcuts.restoreDefault")}
            </Button>
            <div className="flex gap-2">
              <Button variant="outline" onClick={cancelRecording}>
                {t("common.cancel")}
              </Button>
              <Button
                onClick={saveShortcut}
                disabled={
                  !tempShortcut || tempShortcut.includes("...") || loadingSlot !== null
                }
              >
                {t("common.save")}
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Win+V Confirmation Dialog */}
      <Dialog
        open={winvConfirmDialogOpen}
        onOpenChange={(open) => {
          if (!open) {
            setWinvConfirmDialogOpen(false);
            setWinvPendingAction(null);
          }
        }}
      >
        <DialogContent className="max-w-[400px]" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {winvPendingAction === "enable" ? t("settings.shortcuts.enableWinV") : t("settings.shortcuts.disableWinV")}
            </DialogTitle>
            <DialogDescription>
              {winvPendingAction === "enable"
                ? t("settings.shortcuts.enableWinVDesc")
                : t("settings.shortcuts.disableWinVDesc")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setWinvConfirmDialogOpen(false);
                setWinvPendingAction(null);
              }}
            >
              {t("common.cancel")}
            </Button>
            <Button onClick={executeWinvToggle}>{t("common.confirm")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

