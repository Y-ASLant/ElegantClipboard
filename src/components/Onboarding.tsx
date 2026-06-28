import { useState, useCallback } from "react";
import {
  ClipboardMultiple16Regular,
  Search16Regular,
  Pin16Regular,
  Keyboard16Regular,
  CheckmarkCircle16Filled,
} from "@fluentui/react-icons";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface OnboardingProps {
  onComplete: () => void;
}

interface OnboardingStep {
  icon: React.ReactNode;
  title: string;
  description: string;
  tip?: string;
}

const STEPS: OnboardingStep[] = [
  {
    icon: <ClipboardMultiple16Regular className="w-10 h-10 text-primary" />,
    title: "欢迎使用 ElegantClipboard",
    description: "这是一个高效、现代的剪贴板管理工具。所有数据完全离线存储，保护您的隐私。",
    tip: "复制任意内容即可开始记录",
  },
  {
    icon: <Search16Regular className="w-10 h-10 text-primary" />,
    title: "智能搜索",
    description: "使用顶部搜索框快速查找历史记录，支持关键词高亮定位。",
    tip: "窗口显示时搜索框自动聚焦",
  },
  {
    icon: <Pin16Regular className="w-10 h-10 text-primary" />,
    title: "置顶与收藏",
    description: "将重要内容置顶或收藏，方便快速访问。悬停卡片查看操作按钮。",
    tip: "点击卡片即可粘贴到当前窗口",
  },
  {
    icon: <Keyboard16Regular className="w-10 h-10 text-primary" />,
    title: "快捷键操作",
    description: "使用键盘快捷键高效操作：方向键导航、回车粘贴、Delete 删除、左右键切换分类。",
    tip: "Shift+Enter 粘贴为纯文本",
  },
];

export function Onboarding({ onComplete }: OnboardingProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isAnimating, setIsAnimating] = useState(false);

  const handleNext = useCallback(() => {
    if (isAnimating) return;
    setIsAnimating(true);
    
    if (currentStep < STEPS.length - 1) {
      setCurrentStep((prev) => prev + 1);
    } else {
      onComplete();
    }
    
    setTimeout(() => setIsAnimating(false), 300);
  }, [currentStep, isAnimating, onComplete]);

  const handleSkip = useCallback(() => {
    onComplete();
  }, [onComplete]);

  const step = STEPS[currentStep];
  const isLastStep = currentStep === STEPS.length - 1;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div className="w-full max-w-sm mx-4">
        {/* Main card */}
        <div
          className={cn(
            "rounded-xl border bg-card shadow-lg overflow-hidden transition-all duration-300",
            isAnimating && "scale-95 opacity-90"
          )}
        >
          {/* Content */}
          <div className="p-6 text-center">
            {/* Icon */}
            <div className="flex justify-center mb-4">
              <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center">
                {step.icon}
              </div>
            </div>

            {/* Title */}
            <h2 className="text-lg font-semibold mb-2">{step.title}</h2>

            {/* Description */}
            <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
              {step.description}
            </p>

            {/* Tip */}
            {step.tip && (
              <div className="flex items-center justify-center gap-2 text-xs text-primary bg-primary/5 rounded-md px-3 py-2">
                <CheckmarkCircle16Filled className="w-4 h-4 shrink-0" />
                <span>{step.tip}</span>
              </div>
            )}
          </div>

          {/* Progress dots */}
          <div className="flex justify-center gap-2 pb-4">
            {STEPS.map((_, index) => (
              <div
                key={index}
                className={cn(
                  "w-2 h-2 rounded-full transition-all duration-300",
                  index === currentStep
                    ? "bg-primary w-4"
                    : index < currentStep
                    ? "bg-primary/50"
                    : "bg-muted"
                )}
              />
            ))}
          </div>

          {/* Actions */}
          <div className="flex items-center justify-between px-6 pb-6">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleSkip}
              className="text-muted-foreground"
            >
              跳过
            </Button>
            <Button size="sm" onClick={handleNext}>
              {isLastStep ? "开始使用" : "下一步"}
            </Button>
          </div>
        </div>

        {/* Keyboard shortcut hint */}
        <p className="text-center text-xs text-muted-foreground mt-4">
          按 <kbd className="px-1.5 py-0.5 rounded bg-muted text-[10px] font-mono">Esc</kbd> 跳过引导
        </p>
      </div>
    </div>
  );
}
