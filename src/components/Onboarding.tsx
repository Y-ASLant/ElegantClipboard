import { useMemo, useCallback, useState } from "react";
import {
  ClipboardMultiple16Regular,
  Search16Regular,
  Pin16Regular,
  Keyboard16Regular,
  CheckmarkCircle16Filled,
} from "@fluentui/react-icons";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { cn } from "@/lib/utils";

interface OnboardingProps {
  onComplete: () => void;
}

export function Onboarding({ onComplete }: OnboardingProps) {
  const { t } = useTranslation();
  const steps = useMemo(
    () => [
      {
        icon: <ClipboardMultiple16Regular className="w-10 h-10 text-primary" />,
        title: t("onboarding.step1Title"),
        description: t("onboarding.step1Description"),
        tip: t("onboarding.step1Tip"),
      },
      {
        icon: <Search16Regular className="w-10 h-10 text-primary" />,
        title: t("onboarding.step2Title"),
        description: t("onboarding.step2Description"),
        tip: t("onboarding.step2Tip"),
      },
      {
        icon: <Pin16Regular className="w-10 h-10 text-primary" />,
        title: t("onboarding.step3Title"),
        description: t("onboarding.step3Description"),
        tip: t("onboarding.step3Tip"),
      },
      {
        icon: <Keyboard16Regular className="w-10 h-10 text-primary" />,
        title: t("onboarding.step4Title"),
        description: t("onboarding.step4Description"),
        tip: t("onboarding.step4Tip"),
      },
    ],
    [t],
  );

  const [currentStep, setCurrentStep] = useState(0);
  const [isAnimating, setIsAnimating] = useState(false);

  const handleNext = useCallback(() => {
    if (isAnimating) return;
    setIsAnimating(true);

    if (currentStep < steps.length - 1) {
      setCurrentStep((prev) => prev + 1);
    } else {
      onComplete();
    }

    setTimeout(() => setIsAnimating(false), 300);
  }, [currentStep, isAnimating, onComplete, steps.length]);

  const handleSkip = useCallback(() => {
    onComplete();
  }, [onComplete]);

  const step = steps[currentStep];
  const isLastStep = currentStep === steps.length - 1;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div className="w-full max-w-sm mx-4">
        <div
          className={cn(
            "rounded-xl border bg-card shadow-md overflow-hidden transition-all duration-300",
            isAnimating && "scale-95 opacity-90",
          )}
        >
          <div className="p-6 text-center">
            <div className="flex justify-center mb-4">
              <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center">
                {step.icon}
              </div>
            </div>
            <h2 className="text-lg font-semibold mb-2">{step.title}</h2>
            <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
              {step.description}
            </p>
            {step.tip && (
              <div className="flex items-center justify-center gap-2 text-xs text-primary bg-primary/5 rounded-md px-3 py-2">
                <CheckmarkCircle16Filled className="w-4 h-4 shrink-0" />
                <span>{step.tip}</span>
              </div>
            )}
          </div>

          <div className="flex justify-center gap-2 pb-4">
            {steps.map((_, index) => (
              <div
                key={index}
                className={cn(
                  "w-2 h-2 rounded-full transition-all duration-300",
                  index === currentStep
                    ? "bg-primary w-4"
                    : index < currentStep
                      ? "bg-primary/50"
                      : "bg-muted",
                )}
              />
            ))}
          </div>

          <div className="flex items-center justify-between px-6 pb-6">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleSkip}
              className="text-muted-foreground"
            >
              {t("onboarding.skip")}
            </Button>
            <Button size="sm" onClick={handleNext}>
              {isLastStep ? t("onboarding.start") : t("onboarding.next")}
            </Button>
          </div>
        </div>

        <p className="text-center text-xs text-muted-foreground mt-4">
          {t("onboarding.escHint", { key: "Esc" })}
        </p>
      </div>
    </div>
  );
}
