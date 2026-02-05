import * as React from "react"
import * as SwitchPrimitives from "@radix-ui/react-switch"
import { cn } from "@/lib/utils"
import { Checkmark12Filled, Dismiss12Filled } from "@fluentui/react-icons"

interface SwitchProps extends React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root> {
  /** Show indicator icons on thumb */
  showIndicator?: boolean
}

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  SwitchProps
>(({ className, showIndicator = true, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      "group/switch peer inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full",
      "border-2 border-transparent shadow-sm",
      "transition-all duration-200",
      "disabled:cursor-not-allowed disabled:opacity-50",
      "data-[state=checked]:bg-primary data-[state=unchecked]:bg-muted",
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        "pointer-events-none relative flex items-center justify-center",
        "h-5 w-5 rounded-full bg-background shadow-md",
        "ring-0 transition-all duration-200",
        "data-[state=checked]:translate-x-5 data-[state=unchecked]:translate-x-0",
        // Press feedback: shrink on press, expand on release
        "group-active/switch:scale-90 group-active/switch:shadow-sm"
      )}
    >
      {showIndicator && (
        <>
          {/* Check icon - visible when checked */}
          <Checkmark12Filled 
            className={cn(
              "h-3 w-3 text-primary absolute transition-opacity duration-150",
              "opacity-0 group-data-[state=checked]/switch:opacity-100"
            )}
          />
          {/* X icon - visible when unchecked */}
          <Dismiss12Filled 
            className={cn(
              "h-3 w-3 text-muted-foreground absolute transition-opacity duration-150",
              "opacity-100 group-data-[state=checked]/switch:opacity-0"
            )}
          />
        </>
      )}
    </SwitchPrimitives.Thumb>
  </SwitchPrimitives.Root>
))
Switch.displayName = SwitchPrimitives.Root.displayName

export { Switch }
