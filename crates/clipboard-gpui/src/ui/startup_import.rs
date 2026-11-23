//! 启动时若存在 `.pending-import`：先开可见进度窗再解压/导入，避免无界面“假闪退”。

use super::*;
use gpui_kit::component::progress::Progress;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        mpsc::{self, TryRecvError},
    },
    thread,
    time::Duration,
};

#[derive(Clone)]
struct LiveProgress {
    percent: f32,
    label: SharedString,
}

struct StartupImportView {
    title: SharedString,
    detail: SharedString,
    percent: f32,
    error: Option<SharedString>,
    done: bool,
}

impl StartupImportView {
    fn running(kind_label: &str) -> Self {
        Self {
            title: format!("正在{kind_label}").into(),
            detail: "正在准备".into(),
            percent: 0.0,
            error: None,
            done: false,
        }
    }

    fn set_progress(&mut self, percent: f32, label: impl Into<SharedString>) {
        self.percent = percent.clamp(0.0, 100.0);
        self.detail = label.into();
    }

    fn succeed(&mut self, summary: Option<&str>) {
        self.done = true;
        self.percent = 100.0;
        self.title = "导入完成".into();
        self.detail = summary
            .filter(|text| !text.is_empty())
            .map(|text| format!("{text}，即将启动主界面").into())
            .unwrap_or_else(|| "即将启动主界面".into());
        self.error = None;
    }

    fn fail(&mut self, error: impl Into<SharedString>) {
        self.done = true;
        self.title = "导入失败".into();
        self.detail = "窗口将自动关闭，可稍后重试。".into();
        self.error = Some(error.into());
    }
}

impl Render for StartupImportView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running = !self.done && self.error.is_none();
        let percent_label: SharedString = format!("{:.0}%", self.percent).into();
        let ui_size = cx.theme().font_size;
        let title_size = ui_size + px(4.);
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(visual::SPACE_3))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .text_size(ui_size)
            .child(
                div()
                    .text_size(title_size)
                    .font_semibold()
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .w(px(360.))
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_2))
                    .child(
                        Progress::new("startup-import-progress")
                            .value(self.percent)
                            .loading(running && self.percent < 0.5)
                            .w(px(360.))
                            .accessibility_label(self.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(visual::SPACE_2))
                            .text_color(cx.theme().muted_foreground)
                            .child(div().flex_1().child(self.detail.clone()))
                            .child(div().font_semibold().child(percent_label)),
                    ),
            )
            .when_some(self.error.clone(), |col, error| {
                col.child(
                    div()
                        .max_w(px(480.))
                        .px(px(visual::SPACE_4))
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
    }
}

fn load_startup_fonts(data_dir: &Path) -> clipboard_core::preferences::FontPreference {
    clipboard_core::preferences::peek_fonts(&data_dir.join("clipboard.db"))
}

fn kind_label_for(data_dir: &Path) -> &'static str {
    let Ok(bytes) = std::fs::read(data_dir.join(".pending-import")) else {
        return "导入数据";
    };
    match clipboard_platform::PendingImport::try_from_bytes(&bytes)
        .ok()
        .as_ref()
        .map(|job| job.kind.as_str())
    {
        Some("legacy") => "导入旧版数据",
        Some("gpui") => "恢复备份",
        _ => "导入数据",
    }
}

struct SharedImport {
    live: Option<LiveProgress>,
    outcome: Option<anyhow::Result<Option<String>>>,
}

type SharedSlot = Arc<Mutex<SharedImport>>;

fn with_shared(slot: &SharedSlot, f: impl FnOnce(&mut SharedImport)) {
    if let Ok(mut guard) = slot.lock() {
        f(&mut guard);
    }
}

/// 阻塞跑完 pending import（带进度窗）。成功返回结果文案。
pub fn run(data_dir: &Path) -> anyhow::Result<Option<String>> {
    let kind_label = kind_label_for(data_dir);
    let import_dir = data_dir.to_path_buf();
    let shared: SharedSlot = Arc::new(Mutex::new(SharedImport {
        live: Some(LiveProgress {
            percent: 0.0,
            label: "正在准备".into(),
        }),
        outcome: None,
    }));
    let worker_shared = Arc::clone(&shared);
    let (tx, rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        let mut last_bucket = -1i32;
        let result = clipboard_platform::apply_pending_import_with_progress(
            &import_dir,
            &mut |percent, label| {
                let bucket = (percent * 2.0).floor() as i32;
                if bucket == last_bucket {
                    return;
                }
                last_bucket = bucket;
                with_shared(&worker_shared, |state| {
                    state.live = Some(LiveProgress {
                        percent,
                        label: label.to_string().into(),
                    });
                });
            },
        )
        .map_err(|error| format!("{error:#}"));
        with_shared(&worker_shared, |state| {
            state.outcome = Some(match result {
                Ok(message) => Ok(message),
                Err(error) => Err(anyhow::anyhow!(error)),
            });
            state.live = None;
        });
        let _ = tx.send(());
    });

    let fonts = load_startup_fonts(data_dir);
    let outcome_read = Arc::clone(&shared);
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        crate::design_system::set_fonts(fonts, cx);
        let window_bounds = Bounds::centered(None, size(px(520.), px(280.)), cx);
        let shared_window = Arc::clone(&shared);
        let shared_open_err = Arc::clone(&shared);
        if let Err(error) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("ElegantClipboard".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_min_size: Some(size(px(420.), px(220.))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.ElegantClipboard".into()),
                ..WindowOptions::default()
            },
            cx,
            move |_window, cx| {
                let view = cx.new(|_| StartupImportView::running(kind_label));
                let poll = view.clone();
                let shared_poll = Arc::clone(&shared_window);
                cx.spawn(async move |cx| {
                    loop {
                        let snapshot = shared_poll.lock().ok().map(|state| {
                            (
                                state.live.clone(),
                                state.outcome.as_ref().map(|result| match result {
                                    Ok(message) => Ok(message.clone()),
                                    Err(error) => Err(error.to_string()),
                                }),
                            )
                        });
                        let Some((live, outcome)) = snapshot else {
                            break;
                        };
                        if let Some(live) = live {
                            poll.update(cx, |this, cx| {
                                if !this.done {
                                    this.set_progress(live.percent, live.label.clone());
                                    cx.notify();
                                }
                            });
                        }
                        if let Some(outcome) = outcome {
                            let hold_ms = match outcome {
                                Ok(message) => {
                                    poll.update(cx, |this, cx| {
                                        this.succeed(message.as_deref());
                                        cx.notify();
                                    });
                                    800
                                }
                                Err(error) => {
                                    poll.update(cx, |this, cx| {
                                        this.fail(error);
                                        cx.notify();
                                    });
                                    5_000
                                }
                            };
                            cx.background_executor()
                                .timer(Duration::from_millis(hold_ms))
                                .await;
                            cx.update(|cx| cx.quit());
                            return;
                        }
                        match rx.try_recv() {
                            Ok(()) | Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => {
                            }
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(80))
                            .await;
                    }
                    cx.update(|cx| cx.quit());
                })
                .detach();
                view
            },
        ) {
            with_shared(&shared_open_err, |state| {
                state.outcome = Some(Err(anyhow::anyhow!("无法打开导入进度窗口：{error:?}")));
            });
            cx.quit();
        }
    });

    match outcome_read
        .lock()
        .ok()
        .and_then(|mut slot| slot.outcome.take())
    {
        Some(result) => result,
        None => Err(anyhow::anyhow!("导入未完成")),
    }
}
