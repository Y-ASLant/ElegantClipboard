import type { Dispatch, SetStateAction } from "react";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

type SyncTypesSectionProps = {
  syncTypes: Set<string>;
  setSyncTypes: Dispatch<SetStateAction<Set<string>>>;
  maxImageSizeKb: string;
  setMaxImageSizeKb: (value: string) => void;
  maxFileSizeKb: string;
  setMaxFileSizeKb: (value: string) => void;
  maxVideoSizeKb: string;
  setMaxVideoSizeKb: (value: string) => void;
};

const SYNC_TYPES = ["text", "image", "files", "video"] as const;
const SYNC_TYPE_LABELS = { text: "文本", image: "图片", files: "文件", video: "视频" } as const;

const COMMON_SIZE_OPTIONS = [
  ["1024", "1 MB"],
  ["2048", "2 MB"],
  ["5120", "5 MB"],
  ["10240", "10 MB"],
  ["20480", "20 MB"],
  ["51200", "50 MB"],
  ["0", "不限"],
] as const;

const LARGE_SIZE_OPTIONS = [
  ...COMMON_SIZE_OPTIONS.slice(0, -1),
  ["102400", "100 MB"],
  COMMON_SIZE_OPTIONS[COMMON_SIZE_OPTIONS.length - 1],
] as const;

function sizeMbLabel(sizeKb: string) {
  const sizeMb = Math.round(parseInt(sizeKb || "0") / 1024);
  return sizeMb > 0 ? `${sizeMb} MB` : "不限";
}

function SizeLimitRow({
  label,
  description,
  value,
  onChange,
  includeLargeOption = false,
}: {
  label: string;
  description: string;
  value: string;
  onChange: (value: string) => void;
  includeLargeOption?: boolean;
}) {
  const options = includeLargeOption ? LARGE_SIZE_OPTIONS : COMMON_SIZE_OPTIONS;

  return (
    <div className="flex items-center justify-between">
      <div className="space-y-0.5">
        <Label className="text-xs">{label}</Label>
        <p className="text-xs text-muted-foreground">
          仅同步 {sizeMbLabel(value)} 以内的{description}
        </p>
      </div>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
        <SelectContent>
          {options.map(([optionValue, optionLabel]) => (
            <SelectItem key={optionValue} value={optionValue}>{optionLabel}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export function SyncTypesSection({
  syncTypes,
  setSyncTypes,
  maxImageSizeKb,
  setMaxImageSizeKb,
  maxFileSizeKb,
  setMaxFileSizeKb,
  maxVideoSizeKb,
  setMaxVideoSizeKb,
}: SyncTypesSectionProps) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">同步内容类型</h3>
      <p className="text-xs text-muted-foreground mb-4">
        选择要同步的剪贴板记录类型，软件设置始终同步
      </p>
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2">
          {SYNC_TYPES.map((type) => {
            const active = syncTypes.has(type);
            return (
              <button
                key={type}
                type="button"
                onClick={() => {
                  setSyncTypes((prev) => {
                    const next = new Set(prev);
                    if (next.has(type)) {
                      next.delete(type);
                      if (next.size === 0) return prev;
                    } else {
                      next.add(type);
                    }
                    return next;
                  });
                }}
                className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-colors ${
                  active
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-muted/40 text-muted-foreground border-transparent hover:bg-muted"
                }`}
              >
                {SYNC_TYPE_LABELS[type]}
              </button>
            );
          })}
        </div>
        {syncTypes.has("image") && (
          <SizeLimitRow
            label="图片大小限制"
            description="图片"
            value={maxImageSizeKb}
            onChange={setMaxImageSizeKb}
          />
        )}
        {syncTypes.has("files") && (
          <SizeLimitRow
            label="文件大小限制"
            description="文件"
            value={maxFileSizeKb}
            onChange={setMaxFileSizeKb}
            includeLargeOption
          />
        )}
        {syncTypes.has("video") && (
          <SizeLimitRow
            label="视频大小限制"
            description="视频"
            value={maxVideoSizeKb}
            onChange={setMaxVideoSizeKb}
            includeLargeOption
          />
        )}
      </div>
    </div>
  );
}