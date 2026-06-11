import * as React from "react";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

type SettingSectionProps = React.HTMLAttributes<HTMLDivElement> & {
  children: React.ReactNode;
};

export function SettingSection({ className, children, ...props }: SettingSectionProps) {
  return (
    <Card className={className} {...props}>
      <CardContent className="p-4">{children}</CardContent>
    </Card>
  );
}

type SettingRowProps = React.HTMLAttributes<HTMLDivElement> & {
  icon?: React.ReactNode;
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
};

export function SettingRow({
  icon,
  title,
  description,
  action,
  className,
  children,
  ...props
}: SettingRowProps) {
  return (
    <div className={cn("flex items-center justify-between gap-4", className)} {...props}>
      <div className="min-w-0 space-y-0.5">
        <div className="flex items-center gap-2">
          {icon}
          <span className="text-sm font-medium">{title}</span>
        </div>
        {description && (
          <p className="text-xs leading-5 text-muted-foreground">{description}</p>
        )}
        {children}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
