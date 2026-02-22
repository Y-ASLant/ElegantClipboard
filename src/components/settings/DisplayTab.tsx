import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import {
  useUISettings,
  type CardDensity,
  type TimeFormat,
} from "@/stores/ui-settings";

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
  } = useUISettings();

  return (
    <div className="space-y-4">
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
