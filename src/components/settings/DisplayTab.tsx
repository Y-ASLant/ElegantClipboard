import { useMemo, useEffect, useState } from "react";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { restrictToVerticalAxis, restrictToParentElement } from "@dnd-kit/modifiers";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  ReOrderDotsVertical16Regular,
  Delete16Regular,
  LockClosed16Regular,
  Settings16Regular,
  MultiselectLtr16Regular,
  CloudArrowUp16Regular,
  CloudArrowDown16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { useWebDAVAvailable } from "@/hooks/useWebDAVAvailable";
import { useTranslation } from "@/i18n";
import { getToolbarButtonRegistry } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { isWebDAVToolbarButton } from "@/lib/webdav-availability";
import {
  resolveSettingsAccess,
  useUISettings,
  DEFAULT_TOOLBAR_BUTTONS,
  MAX_TOOLBAR_BUTTONS,
  type ToolbarButton,
} from "@/stores/ui-settings";

const TOOLBAR_BUTTON_ICONS: Record<ToolbarButton, React.ComponentType<{ className?: string }>> = {
  clear: Delete16Regular,
  pin: LockClosed16Regular,
  batch: MultiselectLtr16Regular,
  settings: Settings16Regular,
  "webdav-upload": CloudArrowUp16Regular,
  "webdav-download": CloudArrowDown16Regular,
};

const BASE_TOOLBAR_BUTTONS: ToolbarButton[] = ["clear", "pin", "batch", "settings"];
const WEBDAV_TOOLBAR_BUTTONS: ToolbarButton[] = ["webdav-upload", "webdav-download"];

function SortableToolbarItem({
  id,
  icon: Icon,
  label,
  description,
  active,
  onToggle,
  disabled = false,
}: {
  id: ToolbarButton;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  description: string;
  active: boolean;
  onToggle: () => void;
  disabled?: boolean;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id, disabled: !active });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : undefined,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`flex items-center gap-3 px-3 py-2 rounded-md transition-surface ${
        active ? "bg-accent/50" : "opacity-50"
      }`}
    >
      {active ? (
        <button
          {...attributes}
          {...listeners}
          className="w-4 h-4 flex items-center justify-center text-muted-foreground cursor-move shrink-0 touch-none"
        >
          <ReOrderDotsVertical16Regular className="w-4 h-4" />
        </button>
      ) : (
        <Icon className="w-4 h-4 text-muted-foreground shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <div className="text-xs font-medium">{label}</div>
        <div className="text-caption text-muted-foreground truncate">{description}</div>
      </div>
      <Switch
        checked={active}
        onCheckedChange={onToggle}
        disabled={disabled}
        className="shrink-0"
      />
    </div>
  );
}

export function DisplayTab() {
  const { t, locale } = useTranslation();
  const webdavAvailable = useWebDAVAvailable();
  const [trayIconVisible, setTrayIconVisible] = useState<boolean | null>(null);
  const toolbarButtonRegistry = useMemo(() => getToolbarButtonRegistry(), [locale]);
  const allToolbarButtons = useMemo(
    () => (webdavAvailable ? [...BASE_TOOLBAR_BUTTONS, ...WEBDAV_TOOLBAR_BUTTONS] : BASE_TOOLBAR_BUTTONS),
    [webdavAvailable],
  );

  const positionOptions = useMemo(
    () => [
      { value: "auto" as const, label: t("settings.display.hoverPreview.positionAuto") },
      { value: "left" as const, label: t("settings.display.hoverPreview.positionLeft") },
      { value: "right" as const, label: t("settings.display.hoverPreview.positionRight") },
    ],
    [t],
  );
  const sourceDisplayOptions = useMemo(
    () => [
      { value: "both" as const, label: t("settings.display.info.sourceBoth") },
      { value: "name" as const, label: t("settings.display.info.sourceName") },
      { value: "icon" as const, label: t("settings.display.info.sourceIcon") },
    ],
    [t],
  );
  const densityOptions = useMemo(
    () => [
      { value: "compact" as const, label: t("settings.display.preview.densityCompact") },
      { value: "standard" as const, label: t("settings.display.preview.densityStandard") },
      { value: "spacious" as const, label: t("settings.display.preview.densitySpacious") },
    ],
    [t],
  );
  const layoutOptions = useMemo(
    () => [
      { value: "list" as const, label: t("settings.display.preview.layoutList") },
      { value: "masonry" as const, label: t("settings.display.preview.layoutMasonry") },
    ],
    [t],
  );
  const timeFormatOptions = useMemo(
    () => [
      { value: "absolute" as const, label: t("settings.display.info.timeAbsolute") },
      { value: "relative" as const, label: t("settings.display.info.timeRelative") },
    ],
    [t],
  );
  const {
    cardMaxLines, setCardMaxLines,
    imageAutoHeight, setImageAutoHeight,
    imageMaxHeight, setImageMaxHeight,
    showImageFileName, setShowImageFileName,
    imagePreviewEnabled, setImagePreviewEnabled,
    textPreviewEnabled, setTextPreviewEnabled,
    previewUnboundedMode, setPreviewUnboundedMode,
    previewZoomStep, setPreviewZoomStep,
    previewPosition, setPreviewPosition,
    hoverPreviewDelay, setHoverPreviewDelay,
    showTime, setShowTime,
    showCharCount, setShowCharCount,
    showByteSize, setShowByteSize,
    showSourceApp, setShowSourceApp,
    sourceAppDisplay, setSourceAppDisplay,
    cardDensity, setCardDensity,
    listLayout, setListLayout,
    timeFormat, setTimeFormat,
    toolbarButtons, setToolbarButtons,
    showCategoryFilter, setShowCategoryFilter,
    showDragAreaIndicator, setShowDragAreaIndicator,
  } = useUISettings();
  const { settingsButtonCanBeHidden } = resolveSettingsAccess(
    toolbarButtons,
    trayIconVisible === true,
  );
  const anyHoverPreviewEnabled = imagePreviewEnabled || textPreviewEnabled;

  useEffect(() => {
    if (webdavAvailable) return;
    const current = toolbarButtons.filter((button) => !isWebDAVToolbarButton(button));
    if (current.length !== toolbarButtons.length) {
      setToolbarButtons(current);
    }
  }, [webdavAvailable, toolbarButtons, setToolbarButtons]);

  useEffect(() => {
    let cancelled = false;
    invoke<string | null>("get_setting", { key: "tray_icon_visible" })
      .then((value) => {
        if (!cancelled) setTrayIconVisible(value !== "false");
      })
      .catch((error) => {
        logError("Failed to load tray_icon_visible:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 3 } })
  );

  const isButtonActive = (id: ToolbarButton) => toolbarButtons.includes(id);

  const toggleButton = (id: ToolbarButton) => {
    if (id === "settings" && !settingsButtonCanBeHidden) {
      return;
    }
    if (isButtonActive(id)) {
      setToolbarButtons(toolbarButtons.filter((button) => button !== id));
    } else if (toolbarButtons.length < MAX_TOOLBAR_BUTTONS) {
      setToolbarButtons([...toolbarButtons, id]);
    }
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIdx = toolbarButtons.indexOf(active.id as ToolbarButton);
    const newIdx = toolbarButtons.indexOf(over.id as ToolbarButton);
    if (oldIdx < 0 || newIdx < 0) return;
    const next = [...toolbarButtons];
    next.splice(oldIdx, 1);
    next.splice(newIdx, 0, active.id as ToolbarButton);
    setToolbarButtons(next);
  };

  // 排序：激活按钮在前（保持顺序），未激活在后
  const orderedButtons: ToolbarButton[] = [
    ...toolbarButtons,
    ...allToolbarButtons.filter((b) => !toolbarButtons.includes(b)),
  ];

  return (
    <div className="space-y-3">
      {/* Toolbar Buttons Card */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.display.toolbar.title")}
          description={t("settings.display.toolbar.desc", { max: MAX_TOOLBAR_BUTTONS })}
          action={
            <button
              onClick={() => setToolbarButtons([...DEFAULT_TOOLBAR_BUTTONS])}
              className="text-xs text-muted-foreground hover:text-foreground transition-surface"
            >
              {t("settings.display.toolbar.resetDefault")}
            </button>
          }
        />
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          modifiers={[restrictToVerticalAxis, restrictToParentElement]}
          onDragEnd={handleDragEnd}
        >
          <SortableContext items={toolbarButtons} strategy={verticalListSortingStrategy}>
            <div className="space-y-1">
              {orderedButtons.map((id) => {
                const active = isButtonActive(id);
                const meta = toolbarButtonRegistry[id];
                const Icon = TOOLBAR_BUTTON_ICONS[id];
                const toggleDisabled =
                  id === "settings" && !settingsButtonCanBeHidden;
                return (
                  <SortableToolbarItem
                    key={id}
                    id={id}
                    icon={Icon}
                    label={meta.label}
                    description={
                      toggleDisabled
                        ? t("settings.display.toolbar.settingsRequired")
                        : meta.description
                    }
                    active={active}
                    onToggle={() => toggleButton(id)}
                    disabled={toggleDisabled}
                  />
                );
              })}
              <div className="flex items-center justify-between mt-3 pt-3 border-t">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.display.toolbar.categoryFilter")}</Label>
                  <p className="text-xs text-muted-foreground">{t("settings.display.toolbar.categoryFilterDesc")}</p>
                </div>
                <Switch checked={showCategoryFilter} onCheckedChange={setShowCategoryFilter} />
              </div>
            </div>
          </SortableContext>
        </DndContext>
      </SettingsCard>

      {/* Content Preview Card */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.display.preview.title")}
          description={t("settings.display.preview.desc")}
        />
        
        <div className="space-y-4">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-xs">{t("settings.display.preview.maxLines")}</Label>
              <span className="text-xs font-medium tabular-nums">
                {t("common.lines", { count: cardMaxLines })}
              </span>
            </div>
            <Slider
              value={[cardMaxLines]}
              onValueChange={(value) => setCardMaxLines(value[0])}
              min={1}
              max={10}
              step={1}
            />
            <p className="text-xs text-muted-foreground">
              {t("settings.display.preview.maxLinesDesc")}
            </p>
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.preview.density")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.preview.densityDesc")}</p>
            </div>
            <div className="flex gap-1">
              {densityOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setCardDensity(opt.value)}
                  className={`px-2.5 py-1 text-xs rounded-md border transition-surface ${
                    cardDensity === opt.value
                      ? "bg-primary text-primary-foreground border-primary"
                      : "bg-background text-foreground border-input hover:bg-accent"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.preview.layout")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.preview.layoutDesc")}</p>
            </div>
            <div className="flex gap-1">
              {layoutOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setListLayout(opt.value)}
                  className={`px-2.5 py-1 text-xs rounded-md border transition-surface ${
                    listLayout === opt.value
                      ? "bg-primary text-primary-foreground border-primary"
                      : "bg-background text-foreground border-input hover:bg-accent"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.preview.dragIndicator")}</Label>
              <p className="text-xs text-muted-foreground">
                {t("settings.display.preview.dragIndicatorDesc")}
              </p>
            </div>
            <Switch
              checked={showDragAreaIndicator}
              onCheckedChange={setShowDragAreaIndicator}
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.preview.imageAutoHeight")}</Label>
              <p className="text-xs text-muted-foreground">
                {t("settings.display.preview.imageAutoHeightDesc")}
              </p>
            </div>
            <Switch checked={imageAutoHeight} onCheckedChange={setImageAutoHeight} />
          </div>

          {imageAutoHeight && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.display.preview.imageMaxHeight")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {imageMaxHeight} px
                </span>
              </div>
              <Slider
                value={[imageMaxHeight]}
                onValueChange={(value) => setImageMaxHeight(value[0])}
                min={128}
                max={1024}
                step={32}
              />
              <p className="text-xs text-muted-foreground">
                {t("settings.display.preview.imageMaxHeightDesc")}
              </p>
            </div>
          )}

        </div>
      </SettingsCard>

      {/* Hover Preview Card */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.display.hoverPreview.title")}
          description={t("settings.display.hoverPreview.desc")}
        />

        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.hoverPreview.image")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.hoverPreview.imageDesc")}</p>
            </div>
            <Switch checked={imagePreviewEnabled} onCheckedChange={setImagePreviewEnabled} />
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.hoverPreview.text")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.hoverPreview.textDesc")}</p>
            </div>
            <Switch checked={textPreviewEnabled} onCheckedChange={setTextPreviewEnabled} />
          </div>

          {imagePreviewEnabled && (
            <>
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.display.hoverPreview.unbounded")}</Label>
                  <p className="text-xs text-muted-foreground">
                    {t("settings.display.hoverPreview.unboundedDesc")}
                  </p>
                </div>
                <Switch checked={previewUnboundedMode} onCheckedChange={setPreviewUnboundedMode} />
              </div>

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.display.hoverPreview.position")}</Label>
                  <p className="text-xs text-muted-foreground">{t("settings.display.hoverPreview.positionDesc")}</p>
                </div>
                <div className="flex gap-1">
                  {positionOptions.map((opt) => (
                    <button
                      key={opt.value}
                      onClick={() => setPreviewPosition(opt.value)}
                      className={`px-2.5 py-1 text-xs rounded-md border transition-surface ${
                        previewPosition === opt.value
                          ? "bg-primary text-primary-foreground border-primary"
                          : "bg-background text-foreground border-input hover:bg-accent"
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label className="text-xs">{t("settings.display.hoverPreview.zoomStep")}</Label>
                  <span className="text-xs font-medium tabular-nums">
                    {previewZoomStep}%
                  </span>
                </div>
                <Slider
                  value={[previewZoomStep]}
                  onValueChange={(value) => setPreviewZoomStep(value[0])}
                  min={5}
                  max={50}
                  step={5}
                />
                <p className="text-xs text-muted-foreground">
                  {t("settings.display.hoverPreview.zoomStepDesc")}
                </p>
              </div>
            </>
          )}

          {anyHoverPreviewEnabled && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">{t("settings.display.hoverPreview.delay")}</Label>
                <span className="text-xs font-medium tabular-nums">
                  {hoverPreviewDelay} ms
                </span>
              </div>
              <Slider
                value={[hoverPreviewDelay]}
                onValueChange={(value) => setHoverPreviewDelay(value[0])}
                min={128}
                max={1000}
                step={1}
              />
              <p className="text-xs text-muted-foreground">
                {t("settings.display.hoverPreview.delayDesc")}
              </p>
            </div>
          )}
        </div>
      </SettingsCard>

      {/* Info Display Card */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.display.info.title")}
          description={t("settings.display.info.desc")}
        />
        
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.info.showTime")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.info.showTimeDesc")}</p>
            </div>
            <Switch checked={showTime} onCheckedChange={setShowTime} />
          </div>

          {showTime && (
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.display.info.timeFormat")}</Label>
                <p className="text-xs text-muted-foreground">{t("settings.display.info.timeFormatDesc")}</p>
              </div>
              <div className="flex gap-1">
                {timeFormatOptions.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setTimeFormat(opt.value)}
                    className={`px-2.5 py-1 text-xs rounded-md border transition-surface ${
                      timeFormat === opt.value
                        ? "bg-primary text-primary-foreground border-primary"
                        : "bg-background text-foreground border-input hover:bg-accent"
                    }`}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
          )}
          
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.info.showCharCount")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.info.showCharCountDesc")}</p>
            </div>
            <Switch checked={showCharCount} onCheckedChange={setShowCharCount} />
          </div>
          
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.info.showByteSize")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.info.showByteSizeDesc")}</p>
            </div>
            <Switch checked={showByteSize} onCheckedChange={setShowByteSize} />
          </div>
          
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.info.showImageFileName")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.info.showImageFileNameDesc")}</p>
            </div>
            <Switch checked={showImageFileName} onCheckedChange={setShowImageFileName} />
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.display.info.showSourceApp")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.display.info.showSourceAppDesc")}</p>
            </div>
            <Switch checked={showSourceApp} onCheckedChange={setShowSourceApp} />
          </div>

          {showSourceApp && (
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.display.info.sourceDisplay")}</Label>
                <p className="text-xs text-muted-foreground">{t("settings.display.info.sourceDisplayDesc")}</p>
              </div>
              <div className="flex gap-1">
                {sourceDisplayOptions.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setSourceAppDisplay(opt.value)}
                    className={`px-2.5 py-1 text-xs rounded-md border transition-surface ${
                      sourceAppDisplay === opt.value
                        ? "bg-primary text-primary-foreground border-primary"
                        : "bg-background text-foreground border-input hover:bg-accent"
                    }`}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </SettingsCard>

    </div>
  );
}
