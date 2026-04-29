import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useUISettings, type SoundTiming } from "@/stores/ui-settings";

function SoundCard({ title, desc, enabled, onToggle, timing, onTimingChange }: {
  title: string; desc: string;
  enabled: boolean; onToggle: (v: boolean) => void;
  timing: SoundTiming; onTimingChange: (v: SoundTiming) => void;
}) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">{title}</h3>
      <p className="text-xs text-muted-foreground mb-4">{desc}</p>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label className="text-xs">启用</Label>
          <Switch checked={enabled} onCheckedChange={onToggle} />
        </div>
        <div className="flex items-center justify-between">
          <Label className="text-xs">播放时机</Label>
          <Select value={timing} onValueChange={(v) => onTimingChange(v as SoundTiming)} disabled={!enabled}>
            <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="immediate">立即播放</SelectItem>
              <SelectItem value="after_success">成功后播放</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  );
}

export function AudioTab() {
  const {
    copySound, setCopySound, copySoundTiming, setCopySoundTiming,
    pasteSound, setPasteSound, pasteSoundTiming, setPasteSoundTiming,
  } = useUISettings();

  return (
    <div className="space-y-4">
      <SoundCard title="复制音效" desc="监听到新内容复制时播放提示音"
        enabled={copySound} onToggle={setCopySound}
        timing={copySoundTiming} onTimingChange={setCopySoundTiming} />
      <SoundCard title="粘贴音效" desc="点击卡片粘贴时播放提示音"
        enabled={pasteSound} onToggle={setPasteSound}
        timing={pasteSoundTiming} onTimingChange={setPasteSoundTiming} />
    </div>
  );
}
