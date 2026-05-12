import { useEffect, useRef, useState, type RefObject } from "react";
import { Add16Regular } from "@fluentui/react-icons";
import { focusWindowImmediately } from "@/hooks/useInputFocus";
import { cn } from "@/lib/utils";

interface TagPopoverProps {
  popoverRef: RefObject<HTMLDivElement | null>;
  tags: { id: number; name: string }[];
  itemTagIds: Set<number>;
  onToggle: (tagId: number, isAssigned: boolean) => Promise<void>;
  onCreateAndAssign: (name: string) => Promise<void>;
}

export function TagPopover({
  popoverRef,
  tags,
  itemTagIds,
  onToggle,
  onCreateAndAssign,
}: TagPopoverProps) {
  const [newName, setNewName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const t = setTimeout(async () => {
      await focusWindowImmediately();
      inputRef.current?.focus();
    }, 50);
    return () => clearTimeout(t);
  }, []);

  const handleCreate = async () => {
    const trimmed = newName.trim();
    if (!trimmed) return;
    await onCreateAndAssign(trimmed);
    setNewName("");
  };

  return (
    <div
      ref={popoverRef}
      className="absolute right-1 top-0 z-30 w-[170px] rounded-lg border bg-popover shadow-lg animate-in fade-in-0 zoom-in-95 duration-100"
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {tags.length > 0 && (
        <div className="max-h-36 overflow-y-auto p-1">
          {tags.map((tag) => {
            const isAssigned = itemTagIds.has(tag.id);
            return (
              <div
                key={tag.id}
                onClick={() => onToggle(tag.id, isAssigned)}
                className="flex items-center gap-2 px-2 py-1.5 text-xs rounded-md cursor-default hover:bg-accent hover:text-accent-foreground transition-colors duration-100"
              >
                <span className={cn(
                  "w-3.5 h-3.5 shrink-0 flex items-center justify-center rounded border text-[10px] transition-colors duration-100",
                  isAssigned
                    ? "bg-primary border-primary text-primary-foreground"
                    : "border-muted-foreground/30",
                )}>
                  {isAssigned && "✓"}
                </span>
                <span className="truncate">{tag.name}</span>
              </div>
            );
          })}
        </div>
      )}
      <div className={cn("px-1.5 py-1.5 flex items-center gap-1", tags.length > 0 && "border-t")}>
        <input
          ref={inputRef}
          type="text"
          placeholder="新建标签…"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onFocus={() => focusWindowImmediately()}
          onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); e.stopPropagation(); }}
          onKeyUp={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
          className="flex-1 min-w-0 h-6 px-1.5 text-xs rounded-md border bg-background outline-none focus:ring-1 focus:ring-ring transition-shadow"
        />
        <button
          onClick={handleCreate}
          onMouseDown={(e) => e.stopPropagation()}
          disabled={!newName.trim()}
          className="shrink-0 w-5 h-5 flex items-center justify-center rounded-md hover:bg-accent text-primary disabled:opacity-30 transition-colors"
        >
          <Add16Regular className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}