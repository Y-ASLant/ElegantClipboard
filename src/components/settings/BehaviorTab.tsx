import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useUISettings } from "@/stores/ui-settings";

export function BehaviorTab() {
  const {
    copySound, setCopySound,
    pasteSound, setPasteSound,
    pasteCloseWindow, setPasteCloseWindow,
  } = useUISettings();

  return (
    <div className="space-y-4">
      {/* Sound Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">提示音</h3>
        <p className="text-xs text-muted-foreground mb-4">操作时播放简短的反馈音效</p>

        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">复制提示音</Label>
              <p className="text-xs text-muted-foreground">
                监听到新内容复制时播放提示音
              </p>
            </div>
            <Switch checked={copySound} onCheckedChange={setCopySound} />
          </div>
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">粘贴提示音</Label>
              <p className="text-xs text-muted-foreground">
                点击卡片粘贴时播放提示音
              </p>
            </div>
            <Switch checked={pasteSound} onCheckedChange={setPasteSound} />
          </div>
        </div>
      </div>

      {/* Paste Behavior Card */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">粘贴行为</h3>
        <p className="text-xs text-muted-foreground mb-4">配置点击卡片粘贴后的行为</p>

        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">粘贴后关闭窗口</Label>
              <p className="text-xs text-muted-foreground">
                非锁定模式下，粘贴后自动关闭窗口
              </p>
            </div>
            <Switch checked={pasteCloseWindow} onCheckedChange={setPasteCloseWindow} />
          </div>
        </div>
      </div>

    </div>
  );
}
