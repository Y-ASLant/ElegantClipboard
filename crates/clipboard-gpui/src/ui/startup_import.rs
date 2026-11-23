//! 启动时若存在 `.pending-import`：先开可见进度窗再解压/导入，避免无界面“假闪退”。

use super::*;
use gpui_kit::component::spinner::Spinner;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        mpsc::{self, TryRecvError},
    },
    thread,
    time::Duration,
};

struct StartupImportView {
    title: SharedString,
    detail: SharedString,
    error: Option<SharedString>,
    done: bool,
}

impl StartupImportView {
    fn running(kind_label: &str) -> Self {
        Self {
            title: format!("正在{kind_label}").into(),
            detail: "请勿关闭此窗口。大备份可能需要几分钟。".into(),
            error: None,
            done: false,
        }
    }

    fn succeed(&mut self) {
        self.done = true;
        self.title = "导入完成".into();
        self.detail = "即将启动主界面".into();
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
        let spinner = !self.done && self.error.is_none();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(visual::SPACE_3))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(visual::SPACE_2))
                    .when(spinner, |row| {
                        row.child(Spinner::new().color(cx.theme().primary))
                    })
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_semibold()
                            .child(self.title.clone()),
                    ),
            )
            .child(
                div()
                    .max_w(px(420.))
                    .px(px(visual::SPACE_4))
                    .text_size(px(13.))
                    .text_color(cx.theme().muted_foreground)
                    .child(self.detail.clone()),
            )
            .when_some(self.error.clone(), |col, error| {
                col.child(
                    div()
                        .max_w(px(480.))
                        .px(px(visual::SPACE_4))
                        .text_size(px(13.))
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
    }
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

type ImportOutcomeSlot = Arc<Mutex<Option<anyhow::Result<Option<String>>>>>;

fn store_outcome(slot: &ImportOutcomeSlot, value: anyhow::Result<Option<String>>) {
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(value);
    }
}

/// 阻塞跑完 pending import（带进度窗）。成功返回结果文案。
pub fn run(data_dir: &Path) -> anyhow::Result<Option<String>> {
    let kind_label = kind_label_for(data_dir);
    let import_dir = data_dir.to_path_buf();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = clipboard_platform::apply_pending_import(&import_dir)
            .map_err(|error| format!("{error:#}"));
        let _ = tx.send(result);
    });

    let outcome: ImportOutcomeSlot = Arc::new(Mutex::new(None));
    let outcome_read = Arc::clone(&outcome);

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        crate::design_system::apply(cx);
        let window_bounds = Bounds::centered(None, size(px(520.), px(240.)), cx);
        let outcome_window = Arc::clone(&outcome);
        let outcome_open_err = Arc::clone(&outcome);
        if let Err(error) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("ElegantClipboard".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_min_size: Some(size(px(420.), px(200.))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.ElegantClipboard".into()),
                ..WindowOptions::default()
            },
            cx,
            move |_window, cx| {
                let view = cx.new(|_| StartupImportView::running(kind_label));
                let poll = view.clone();
                let outcome_poll = Arc::clone(&outcome_window);
                cx.spawn(async move |cx| {
                    let import_result = loop {
                        match rx.try_recv() {
                            Ok(result) => break result,
                            Err(TryRecvError::Empty) => {
                                cx.background_executor()
                                    .timer(Duration::from_millis(120))
                                    .await;
                            }
                            Err(TryRecvError::Disconnected) => {
                                break Err("导入线程意外结束".into());
                            }
                        }
                    };

                    let hold_ms = match &import_result {
                        Ok(message) => {
                            store_outcome(&outcome_poll, Ok(message.clone()));
                            poll.update(cx, |this, cx| {
                                this.succeed();
                                cx.notify();
                            });
                            700
                        }
                        Err(error) => {
                            store_outcome(&outcome_poll, Err(anyhow::anyhow!(error.clone())));
                            let error = error.clone();
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
                })
                .detach();
                view
            },
        ) {
            store_outcome(
                &outcome_open_err,
                Err(anyhow::anyhow!("无法打开导入进度窗口：{error:?}")),
            );
            cx.quit();
        }
    });

    match outcome_read.lock().ok().and_then(|mut slot| slot.take()) {
        Some(result) => result,
        None => Err(anyhow::anyhow!("导入未完成")),
    }
}
