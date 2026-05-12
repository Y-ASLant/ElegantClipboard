import {
  ClipboardMultiple16Regular,
  Search16Regular,
} from "@fluentui/react-icons";

interface ClipboardListEmptyStatesProps {
  isLoading: boolean;
  hasItems: boolean;
  searchQuery: string;
  onResetView: () => void;
}

export function ClipboardListEmptyStates({
  isLoading,
  hasItems,
  searchQuery,
  onResetView,
}: ClipboardListEmptyStatesProps) {
  if (hasItems) return null;

  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-3">
          <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mx-auto" />
          <p className="text-sm text-muted-foreground">加载中...</p>
        </div>
      </div>
    );
  }

  if (searchQuery) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
            <Search16Regular className="w-8 h-8 text-muted-foreground" />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">未找到匹配的内容</p>
            <p className="text-sm text-muted-foreground">试试其他关键词</p>
          </div>
          <button
            onClick={onResetView}
            className="text-xs text-primary hover:text-primary/80 hover:underline transition-colors"
          >
            清除筛选
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex items-center justify-center h-full">
      <div className="text-center space-y-4">
        <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
          <ClipboardMultiple16Regular className="w-8 h-8 text-muted-foreground" />
        </div>
        <div className="space-y-1">
          <p className="text-sm font-medium">暂无剪贴板历史</p>
          <p className="text-sm text-muted-foreground">复制任意内容开始记录</p>
        </div>
      </div>
    </div>
  );
}