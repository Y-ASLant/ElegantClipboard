import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

type AutoSyncSectionProps = {
  autoSync: boolean;
  setAutoSync: (value: boolean) => void;
  syncInterval: string;
  setSyncInterval: (value: string) => void;
};

export function AutoSyncSection({
  autoSync,
  setAutoSync,
  syncInterval,
  setSyncInterval,
}: AutoSyncSectionProps) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">自动同步</h3>
      <p className="text-xs text-muted-foreground mb-4">
        定时自动同步，保持多设备数据一致
      </p>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">自动同步</Label>
            <p className="text-xs text-muted-foreground">
              启用后按照设定间隔自动执行同步
            </p>
          </div>
          <Switch checked={autoSync} onCheckedChange={setAutoSync} />
        </div>
        {autoSync && (
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">同步间隔</Label>
              <p className="text-xs text-muted-foreground">
                每隔多长时间自动同步一次
              </p>
            </div>
            <Select value={syncInterval} onValueChange={setSyncInterval}>
              <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="30">30 秒</SelectItem>
                <SelectItem value="60">1 分钟</SelectItem>
                <SelectItem value="180">3 分钟</SelectItem>
                <SelectItem value="300">5 分钟</SelectItem>
                <SelectItem value="600">10 分钟</SelectItem>
                <SelectItem value="1200">20 分钟</SelectItem>
              </SelectContent>
            </Select>
          </div>
        )}
      </div>
    </div>
  );
}