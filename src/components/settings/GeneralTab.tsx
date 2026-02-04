import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Folder16Regular, Open16Regular } from "@fluentui/react-icons";

export interface GeneralSettings {
  data_path: string;
  max_history_count: number;
  max_content_size_kb: number;
  auto_start: boolean;
}

interface GeneralTabProps {
  settings: GeneralSettings;
  onSettingsChange: (settings: GeneralSettings) => void;
}

export function GeneralTab({ settings, onSettingsChange }: GeneralTabProps) {
  const selectFolder = async () => {
    try {
      const path = await invoke<string | null>("select_folder_for_settings");
      if (path) {
        onSettingsChange({ ...settings, data_path: path });
      }
    } catch (error) {
      console.error("Failed to select folder:", error);
    }
  };

  const openDataFolder = async () => {
    try {
      await invoke("open_data_folder");
    } catch (error) {
      console.error("Failed to open folder:", error);
    }
  };

  return (
    <div className="space-y-6">
      {/* Storage Path */}
      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">数据存储</h3>
          <p className="text-xs text-muted-foreground">配置剪贴板数据的存储位置</p>
        </div>
        <div className="space-y-2">
          <Label htmlFor="data-path" className="text-xs">存储路径</Label>
          <div className="flex gap-2">
            <Input
              id="data-path"
              value={settings.data_path}
              onChange={(e) => onSettingsChange({ ...settings, data_path: e.target.value })}
              placeholder="加载中..."
              readOnly
              className="flex-1 h-8 text-sm"
            />
            <Button variant="outline" size="icon" onClick={selectFolder} className="h-8 w-8">
              <Folder16Regular className="w-4 h-4" />
            </Button>
            <Button variant="outline" size="icon" onClick={openDataFolder} className="h-8 w-8">
              <Open16Regular className="w-4 h-4" />
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            留空使用默认路径，修改后需重启应用生效
          </p>
        </div>
      </div>

      <Separator />

      {/* History Limit */}
      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">历史记录</h3>
          <p className="text-xs text-muted-foreground">配置历史记录的存储限制</p>
        </div>
        
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Label className="text-xs">最大历史记录数</Label>
            <span className="text-xs font-medium tabular-nums">
              {settings.max_history_count === 0 ? "无限制" : settings.max_history_count.toLocaleString()}
            </span>
          </div>
          <Slider
            value={[settings.max_history_count]}
            onValueChange={(value) => onSettingsChange({ ...settings, max_history_count: value[0] })}
            min={0}
            max={10000}
            step={100}
          />
          <p className="text-xs text-muted-foreground">
            设为 0 表示无限制
          </p>
        </div>

        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Label className="text-xs">单条内容最大大小</Label>
            <span className="text-xs font-medium tabular-nums">
              {settings.max_content_size_kb >= 1024 
                ? `${(settings.max_content_size_kb / 1024).toFixed(1)} MB`
                : `${settings.max_content_size_kb} KB`
              }
            </span>
          </div>
          <Slider
            value={[settings.max_content_size_kb]}
            onValueChange={(value) => onSettingsChange({ ...settings, max_content_size_kb: value[0] })}
            min={64}
            max={10240}
            step={64}
          />
          <p className="text-xs text-muted-foreground">
            超过此大小的内容将被截断保存
          </p>
        </div>
      </div>

      <Separator />

      {/* Startup */}
      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">启动</h3>
          <p className="text-xs text-muted-foreground">配置应用启动行为</p>
        </div>
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">开机自启动</Label>
            <p className="text-xs text-muted-foreground">
              系统启动时自动运行
            </p>
          </div>
          <Switch
            checked={settings.auto_start}
            onCheckedChange={(checked) => onSettingsChange({ ...settings, auto_start: checked })}
          />
        </div>
      </div>
    </div>
  );
}
