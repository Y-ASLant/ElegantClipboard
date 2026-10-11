use super::super::CapturedClipboard;
use super::Worker;
use crate::source_app;
use anyhow::Result;
use clipboard_core::{is_url_text, preferences::MonitorTypesPreference};

impl Worker {
    pub(super) fn handle_observed_capture(
        &mut self,
        content: CapturedClipboard,
        source: Option<source_app::SourceApp>,
    ) -> Result<()> {
        let excluded = self.app_filter.excludes(
            source.as_ref().map(|app| app.name.as_str()),
            source.as_ref().and_then(|app| app.executable.as_deref()),
        );
        let filtered = (!excluded)
            .then(|| filter_observed_capture(content, self.monitor_types))
            .flatten();
        let Some(content) = filtered else {
            return Ok(());
        };
        let id =
            match content {
                CapturedClipboard::Text(text) => {
                    self.history.capture_with_media(&text, &self.images_dir)?
                }
                CapturedClipboard::Rich { html, rtf, text } => Some(self.history.capture_rich(
                    html.as_deref(),
                    rtf.as_deref(),
                    text.as_deref(),
                    &self.images_dir,
                )?),
                CapturedClipboard::Files(paths) => {
                    Some(self.history.capture_files(&paths, &self.images_dir)?)
                }
                CapturedClipboard::Image { png, width, height } => Some(
                    self.history
                        .capture_image(&png, width, height, &self.images_dir)?,
                ),
            };
        if let (Some(id), Some(source)) = (id, source) {
            let icon = source.executable.as_deref().and_then(|executable| {
                source_app::extract_and_cache_icon(executable, &self.icons_dir)
            });
            self.history
                .set_source_app(id, &source.name, icon.as_deref())?;
        }
        Ok(())
    }
}
pub(in crate::windows) fn filter_observed_capture(
    content: CapturedClipboard,
    allowed: MonitorTypesPreference,
) -> Option<CapturedClipboard> {
    match content {
        CapturedClipboard::Text(text) => {
            let enabled = if is_url_text(&text) {
                allowed.url
            } else {
                allowed.text
            };
            enabled.then_some(CapturedClipboard::Text(text))
        }
        CapturedClipboard::Rich { html, rtf, text } => {
            let html = html.filter(|_| allowed.html);
            let rtf = rtf.filter(|_| allowed.rtf);
            if html.is_some() || rtf.is_some() {
                Some(CapturedClipboard::Rich { html, rtf, text })
            } else {
                text.and_then(|text| {
                    filter_observed_capture(CapturedClipboard::Text(text), allowed)
                })
            }
        }
        CapturedClipboard::Image { .. } if !allowed.image => None,
        CapturedClipboard::Files(_) if !allowed.files => None,
        content => Some(content),
    }
}
