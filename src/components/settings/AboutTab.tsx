import {
  Person16Regular,
  Code16Regular,
  Star16Regular,
  Alert16Regular,
} from "@fluentui/react-icons";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { useTranslation } from "@/i18n";
import { reportUserError } from "@/lib/operation-feedback";

export function AboutTab() {
  const { t } = useTranslation();

  const openUrl = async (url: string) => {
    try {
      await tauriOpenUrl(url);
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.openLink"), error, "Failed to open URL");
    }
  };

  return (
    <>
      <div className="flex-1 rounded-md border bg-card p-6 flex flex-col justify-center overflow-auto">
        <div className="flex flex-col items-center text-center space-y-4">
          <div className="h-16 w-16 rounded-md overflow-hidden">
            <img
              src="/icon.png"
              alt="ElegantClipboard"
              className="w-full h-full object-contain"
            />
          </div>
          <h3 className="font-semibold text-lg">ElegantClipboard</h3>
          <p className="text-sm text-muted-foreground max-w-xs">
            {t("settings.about.tagline")}
          </p>
        </div>
      </div>

      <SettingsCard className="flex-1 flex flex-col overflow-auto">
        <SettingsCardHeader title={t("settings.about.authorTitle")} />
        <div className="space-y-2 flex-1 flex flex-col justify-center">
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Person16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">{t("settings.about.author")}</span>
            </div>
            <span className="text-sm font-medium text-primary">ASLant</span>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Code16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">GitHub</span>
            </div>
            <button
              onClick={() => openUrl("https://github.com/Y-ASLant")}
              className="text-sm font-medium text-primary hover:underline"
            >
              @Y-ASLant
            </button>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Star16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">{t("settings.about.star")}</span>
            </div>
            <button
              onClick={() =>
                openUrl("https://github.com/Y-ASLant/ElegantClipboard")
              }
              className="text-sm font-medium text-primary hover:underline"
            >
              ElegantClipboard
            </button>
          </div>
          <div className="flex items-center justify-between py-1.5">
            <div className="flex items-center gap-2">
              <Alert16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm text-muted-foreground">{t("settings.about.feedback")}</span>
            </div>
            <button
              onClick={() =>
                openUrl(
                  "https://github.com/Y-ASLant/ElegantClipboard/issues",
                )
              }
              className="text-sm font-medium text-primary hover:underline"
            >
              {t("settings.about.submitIssue")}
            </button>
          </div>
        </div>
      </SettingsCard>

      <div className="shrink-0 py-1 text-center">
        <button
          type="button"
          onClick={() => openUrl("https://github.com/Y-ASLant/ElegantClipboard/tree/gpui")}
          className="text-sm font-medium text-primary hover:underline"
        >
          {t("settings.about.gpuiRewrite")}
        </button>
      </div>
    </>
  );
}
