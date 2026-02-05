import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";

interface DisplayTabProps {
  cardMaxLines: number;
  setCardMaxLines: (value: number) => void;
  showTime: boolean;
  setShowTime: (value: boolean) => void;
  showCharCount: boolean;
  setShowCharCount: (value: boolean) => void;
  showByteSize: boolean;
  setShowByteSize: (value: boolean) => void;
}

export function DisplayTab({
  cardMaxLines,
  setCardMaxLines,
  showTime,
  setShowTime,
  showCharCount,
  setShowCharCount,
  showByteSize,
  setShowByteSize,
}: DisplayTabProps) {
  return (
    <div className="space-y-6">
      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">内容预览</h3>
          <p className="text-xs text-muted-foreground">配置剪贴板卡片的内容显示</p>
        </div>
        
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
      </div>

      <Separator />

      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">信息显示</h3>
          <p className="text-xs text-muted-foreground">配置卡片底部显示的信息</p>
        </div>
        
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">显示时间</Label>
              <p className="text-xs text-muted-foreground">显示复制的具体时间</p>
            </div>
            <Switch checked={showTime} onCheckedChange={setShowTime} />
          </div>
          
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
        </div>
      </div>
    </div>
  );
}
