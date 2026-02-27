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
} from "@fluentui/react-icons";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { TOOLBAR_BUTTON_REGISTRY } from "@/lib/constants";
import {
  useUISettings,
  DEFAULT_TOOLBAR_BUTTONS,
  MAX_TOOLBAR_BUTTONS,
  type CardDensity,
  type TimeFormat,
  type ToolbarButton,
} from "@/stores/ui-settings";

const TOOLBAR_BUTTON_ICONS: Record<ToolbarButton, React.ComponentType<{ className?: string }>> = {
  clear: Delete16Regular,
  pin: LockClosed16Regular,
  settings: Settings16Regular,
};

const ALL_TOOLBAR_BUTTONS: ToolbarButton[] = Object.keys(TOOLBAR_BUTTON_REGISTRY) as ToolbarButton[];

function SortableToolbarItem({
  id,
  icon: Icon,
  label,
  description,
  active,
  onToggle,
}: {
  id: ToolbarButton;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  description: string;
  active: boolean;
  onToggle: () => void;
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
      className={`flex items-center gap-3 px-3 py-2 rounded-md transition-colors ${
        active ? "bg-accent/50" : "opacity-50"
      }`}
    >
      {active ? (
        <button
          {...attributes}
          {...listeners}
          className="w-4 h-4 flex items-center justify-center text-muted-foreground cursor-grab active:cursor-grabbing shrink-0 touch-none"
        >
          <ReOrderDotsVertical16Regular className="w-4 h-4" />
        </button>
      ) : (
        <Icon className="w-4 h-4 text-muted-foreground shrink-0" />
      )}
      <div className="flex-1 min-w-0">
        <div className="text-xs font-medium">{label}</div>
        <div className="text-[11px] text-muted-foreground truncate">{description}</div>
      </div>
      <Switch
        checked={active}
        onCheckedChange={onToggle}
        className="shrink-0"
      />
    </div>
  );
}

const positionOptions: { value: "auto" | "left" | "right"; label: string }[] = [
  { value: "auto", label: "自动" },
  { value: "left", label: "左侧" },
  { value: "right", label: "右侧" },
];

const sourceDisplayOptions: { value: "both" | "name" | "icon"; label: string }[] = [
  { value: "both", label: "完整" },
  { value: "name", label: "仅名称" },
  { value: "icon", label: "仅图标" },
];

const densityOptions: { value: CardDensity; label: string }[] = [
  { value: "compact", label: "紧凑" },
  { value: "standard", label: "标准" },
  { value: "spacious", label: "宽松" },
];

const timeFormatOptions: { value: TimeFormat; label: string }[] = [
  { value: "absolute", label: "绝对时间" },
  { value: "relative", label: "相对时间" },
];

export function DisplayTab() {
  const {
    cardMaxLines, setCardMaxLines,
    imageAutoHeight, setImageAutoHeight,
    imageMaxHeight, setImageMaxHeight,
    imagePreviewEnabled, setImagePreviewEnabled,
    previewZoomStep, setPreviewZoomStep,
    previewPosition, setPreviewPosition,
    hoverPreviewDelay, setHoverPreviewDelay,
    showTime, setShowTime,
    showCharCount, setShowCharCount,
    showByteSize, setShowByteSize,
    showSourceApp, setShowSourceApp,
    sourceAppDisplay, setSourceAppDisplay,
    cardDensity, setCardDensity,
    timeFormat, setTimeFormat,
    toolbarButtons, setToolbarButtons,
    showCategoryFilter, setShowCategoryFilter,
  } = useUISettings();

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 3 } })
  );

  const isButtonActive = (id: ToolbarButton) => toolbarButtons.includes(id);

  const toggleButton = (id: ToolbarButton) => {
    if (isButtonActive(id)) {
      setToolbarButtons(toolbarButtons.filter((b) => b !== id));
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

  // Ordered list: active buttons first (in their order), then inactive ones
  const orderedButtons: ToolbarButton[] = [
    ...toolbarButtons,
    ...ALL_TOOLBAR_BUTTONS.filter((b) => !toolbarButtons.includes(b)),
  ];

  return (
    <div className="space-y-4">
      {/* Toolbar Buttons Card */}
      <div className="rounded-lg border bg-card p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-medium">工具栏</h3>
          <button
            onClick={() => setToolbarButtons([...DEFAULT_TOOLBAR_BUTTONS])}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            重置默认
          </button>
        </div>
        <p className="text-xs text-muted-foreground mb-4">
          自定义工具栏显示的按钮及顺序（最多 {MAX_TOOLBAR_BUTTONS} 个）
        </p>
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
                const meta = TOOLBAR_BUTTON_REGISTRY[id];
                const Icon = TOOLBAR_BUTTON_ICONS[id];
                return (
                  <SortableToolbarItem
                    key={id}
                    id={id}
                    icon={Icon}
                    label={meta.label}
                    description={meta.description}
                    active={active}
                    onToggle={() => toggleButton(id)}
                  />
                );
              })}
              <div className="flex items-center justify-between mt-3 pt-3 border-t">
                <div className="space-y-0.5">
                  <Label className="text-xs">底部分类栏</Label>
                  <p className="text-xs text-muted-foreground">显示底部内容类型分类筛选栏</p>
                </div>
                <Switch checked={showCategoryFilter} onCheckedChange={setShowCategoryFilter} />
              </div>
            </div>
          </SortableContext>
        </DndContext>
      </div>

      {/* Content Preview Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">内容预览</h3>
        <p className="text-xs text-muted-foreground mb-4">配置剪贴板卡片的内容显示</p>
        
        <div className="space-y-4">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-xs">预览最大行数</Label>
              <span className="text-xs font-medium tabular-nums">
                {cardMaxLines} 行
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
              超过此行数的内容将被截断显示，内容不足时按实际高度显示
            </p>
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">卡片间距</Label>
              <p className="text-xs text-muted-foreground">调整卡片之间的间距大小</p>
            </div>
            <div className="flex gap-1">
              {densityOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setCardDensity(opt.value)}
                  className={`px-2.5 py-1 text-xs rounded-md border transition-colors ${
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
              <Label className="text-xs">图片自适应高度</Label>
              <p className="text-xs text-muted-foreground">
                关闭后图片高度跟随预览最大行数
              </p>
            </div>
            <Switch checked={imageAutoHeight} onCheckedChange={setImageAutoHeight} />
          </div>

          {imageAutoHeight && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label className="text-xs">图片最大高度</Label>
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
                自适应模式下图片的最大显示高度
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Image Preview Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">图片预览</h3>
        <p className="text-xs text-muted-foreground mb-4">鼠标悬停时在窗口旁显示大图预览</p>

        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">启用图片悬浮预览</Label>
              <p className="text-xs text-muted-foreground">悬停后弹出预览窗口，Ctrl+滚轮缩放</p>
            </div>
            <Switch checked={imagePreviewEnabled} onCheckedChange={setImagePreviewEnabled} />
          </div>

          {imagePreviewEnabled && (
            <>
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">预览位置</Label>
                  <p className="text-xs text-muted-foreground">预览窗口显示在主窗口的哪一侧</p>
                </div>
                <div className="flex gap-1">
                  {positionOptions.map((opt) => (
                    <button
                      key={opt.value}
                      onClick={() => setPreviewPosition(opt.value)}
                      className={`px-2.5 py-1 text-xs rounded-md border transition-colors ${
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
                  <Label className="text-xs">悬浮延迟</Label>
                  <span className="text-xs font-medium tabular-nums">
                    {hoverPreviewDelay} ms
                  </span>
                </div>
                <Slider
                  value={[hoverPreviewDelay]}
                  onValueChange={(value) => setHoverPreviewDelay(value[0])}
                  min={100}
                  max={1000}
                  step={50}
                />
                <p className="text-xs text-muted-foreground">
                  鼠标悬停多久后弹出预览窗口
                </p>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label className="text-xs">缩放步进</Label>
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
                  每次 Ctrl+滚轮缩放的幅度
                </p>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Info Display Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">信息显示</h3>
        <p className="text-xs text-muted-foreground mb-4">配置卡片底部显示的信息</p>
        
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">显示时间</Label>
              <p className="text-xs text-muted-foreground">显示复制的具体时间</p>
            </div>
            <Switch checked={showTime} onCheckedChange={setShowTime} />
          </div>

          {showTime && (
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">时间格式</Label>
                <p className="text-xs text-muted-foreground">选择时间的显示方式</p>
              </div>
              <div className="flex gap-1">
                {timeFormatOptions.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setTimeFormat(opt.value)}
                    className={`px-2.5 py-1 text-xs rounded-md border transition-colors ${
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
              <Label className="text-xs">显示字符数</Label>
              <p className="text-xs text-muted-foreground">显示文本内容的字符数</p>
            </div>
            <Switch checked={showCharCount} onCheckedChange={setShowCharCount} />
          </div>
          
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">显示大小</Label>
              <p className="text-xs text-muted-foreground">显示内容的字节大小</p>
            </div>
            <Switch checked={showByteSize} onCheckedChange={setShowByteSize} />
          </div>
          
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">显示复制来源</Label>
              <p className="text-xs text-muted-foreground">显示复制内容的来源应用</p>
            </div>
            <Switch checked={showSourceApp} onCheckedChange={setShowSourceApp} />
          </div>

          {showSourceApp && (
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">显示方式</Label>
                <p className="text-xs text-muted-foreground">选择显示图标、名称或两者都显示</p>
              </div>
              <div className="flex gap-1">
                {sourceDisplayOptions.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setSourceAppDisplay(opt.value)}
                    className={`px-2.5 py-1 text-xs rounded-md border transition-colors ${
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
      </div>

    </div>
  );
}
