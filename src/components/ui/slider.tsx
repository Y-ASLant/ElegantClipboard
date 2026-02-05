import * as React from "react"
import * as SliderPrimitive from "@radix-ui/react-slider"

import { cn } from "@/lib/utils"

interface SliderProps extends React.ComponentPropsWithoutRef<typeof SliderPrimitive.Root> {
  /** Visual style variant */
  variant?: "outline" | "solid"
}

const Slider = React.forwardRef<
  React.ElementRef<typeof SliderPrimitive.Root>,
  SliderProps
>(({ className, variant = "solid", ...props }, ref) => (
  <SliderPrimitive.Root
    ref={ref}
    className={cn(
      "relative flex w-full touch-none select-none items-center py-1",
      className
    )}
    {...props}
  >
    <SliderPrimitive.Track
      className={cn(
        "relative h-2 w-full grow overflow-hidden rounded-full",
        variant === "outline"
          ? "bg-transparent border-2 border-muted-foreground/30"
          : "bg-muted"
      )}
    >
      <SliderPrimitive.Range
        className={cn(
          "absolute h-full rounded-full",
          variant === "outline"
            ? "bg-primary -my-[2px] my-0 h-[calc(100%+4px)]"
            : "bg-primary"
        )}
      />
    </SliderPrimitive.Track>
    <SliderPrimitive.Thumb
      className={cn(
        "block h-4 w-4 rounded-full bg-background border-2 border-primary",
        "shadow-md shadow-black/20",
        "transition-all duration-150",
        "hover:scale-110 hover:shadow-lg",
        "disabled:pointer-events-none disabled:opacity-50",
        "cursor-grab active:cursor-grabbing active:scale-105"
      )}
    />
  </SliderPrimitive.Root>
))
Slider.displayName = SliderPrimitive.Root.displayName

export { Slider }
