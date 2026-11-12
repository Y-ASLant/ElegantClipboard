use super::*;
#[cfg(not(target_os = "macos"))]
use clipboard_platform::key_intercept::{self, InterceptedKey};

const ACTIVATION_POLL: Duration = Duration::from_millis(20);
const ACTIVATION_ATTEMPTS: usize = 25;

impl ClipboardView {
    #[cfg(not(target_os = "macos"))]
    pub(super) fn key_intercept_events(
        receiver: async_channel::Receiver<InterceptedKey>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let main_window = window.window_handle();
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(key) = receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    this.handle_intercepted_key(key, window, cx);
                })
                .is_err()
                {
                    return;
                }
            }
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_intercepted_key(
        &mut self,
        key: InterceptedKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match key {
            InterceptedKey::Shortcut {
                key,
                control,
                shift,
            } => {
                let keystroke = gpui_kit::Keystroke {
                    modifiers: gpui_kit::Modifiers {
                        control,
                        shift,
                        ..Default::default()
                    },
                    key: key.into(),
                    key_char: None,
                };
                window.dispatch_keystroke(keystroke, cx);
            }
            InterceptedKey::Text => {
                if self.typing_activation.is_some() {
                    return;
                }
                if !self.onboarding_visible() && self.editor.id.is_none() {
                    self.search.update(cx, |input, cx| input.focus(window, cx));
                }
                self.activate_for_input(window, cx);
            }
        }
    }

    /// Text input needs real focus for the IME; navigation works without it.
    pub(super) fn activate_for_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() || self.typing_activation.is_some() {
            return;
        }
        native_window::activate_for_input(window);
        let main = window.window_handle();
        self.typing_activation = Some(cx.spawn_in(window, async move |view, cx| {
            let mut active = false;
            for _ in 0..ACTIVATION_ATTEMPTS {
                cx.background_executor().timer(ACTIVATION_POLL).await;
                active = update_clipboard_window(&view, main, cx, |_, window, _| {
                    window.is_window_active()
                })
                .unwrap_or(false);
                if active {
                    break;
                }
            }
            #[cfg(not(target_os = "macos"))]
            if active {
                if let Err(error) = key_intercept::replay_pending_text() {
                    tracing::warn!(%error, "Could not replay typed keys");
                }
            } else {
                key_intercept::discard_pending_text();
            }
            #[cfg(target_os = "macos")]
            let _ = active;
            let _ = view.update(cx, |this, _| this.typing_activation = None);
        }));
    }
}
