use super::*;

/// 悬停预览打开后固定存活多久。
///
/// 用户要求：**5 秒就关，不看有没有在操作**。所以这里不做「鼠标活动续命」——
/// 浮窗最多存在 5 秒，避免它长时间盖住任务栏之类的可点区域。
/// 需要再看一次就重新悬停一下。
const HOVER_AUTOCLOSE: Duration = Duration::from_secs(5);

/// 鼠标离开「来源行」之后，宽限多久才真正关掉浮窗。
///
/// 这段时间是留给用户把鼠标从行上移到浮窗里的（浮窗在光标旁边 18px 处，移过去要花点时间）。
/// 原来是 180ms，太短：经常还没挪到浮窗上就被关掉了，导致缩放/拖拽按钮根本点不到。
/// 只要在这段时间内鼠标进了浮窗，`set_hover_popup_active` 就会取消这次关闭。
const HOVER_LEAVE_GRACE: Duration = Duration::from_millis(600);

impl ClipboardView {
    /// 给刚打开的悬停预览挂上固定 5 秒的自动关闭计时。
    pub(super) fn arm_hover_autoclose(&mut self, cx: &mut Context<Self>) {
        self.hover_autoclose_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(HOVER_AUTOCLOSE).await;
            let _ = view.update(cx, |this, cx| this.close_hover_preview(cx));
        }));
    }

    pub(super) fn apply_hover_preview_result(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.hover_preview.apply(id, generation, result)
            && self.hover_source_active
            && self.preview.id.is_none()
        {
            self.open_hover_popup(window, cx);
        }
        true
    }

    pub(super) fn apply_hover_preview_saved(
        &mut self,
        result: Result<HoverPreviewPreference, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.hover_preference_pending = false;
        match result {
            Ok(preference) => {
                self.hover_preference = preference;
                self.close_hover_preview(cx);
                self.set_feedback_message(
                    tr(
                        self.language,
                        "悬停预览设置已保存",
                        "Hover preview preferences saved",
                    )
                    .into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "悬停预览设置保存失败",
                        "Failed to save hover preview preferences",
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn close_hover_preview(&mut self, cx: &mut Context<Self>) {
        self.hover_task = None;
        self.hover_close_task = None;
        self.hover_autoclose_task = None;
        self.hover_preview.close();
        self.hover_popup_opening = None;
        self.hover_source_active = false;
        self.hover_popup_active = false;
        if let Some(handle) = self.hover_popup.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        cx.notify();
    }

    pub(super) fn schedule_hover_close(&mut self, cx: &mut Context<Self>) {
        self.hover_close_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(HOVER_LEAVE_GRACE).await;
            let _ = view.update(cx, |this, cx| {
                if !this.hover_source_active && !this.hover_popup_active {
                    this.close_hover_preview(cx);
                }
            });
        }));
    }

    pub(super) fn set_hover_source(&mut self, id: i64, entered: bool, cx: &mut Context<Self>) {
        if entered {
            if self.batch_mode
                || cx.has_active_drag()
                || self.external_drag.is_some()
                || self.history.loading
                || self.preview.id.is_some()
                || self.pending_row_click.is_some()
            {
                return;
            }
            let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
                return;
            };
            // 「文件」记录若恰好是一张图片文件，按「图片」开关判定。复制截图（PixPin 等
            // 工具）时剪贴板里放的是图片**文件**而不是 image 记录，若按「文件」开关判定，
            // 用户明明开了「图片」却看不到任何预览。
            let content_kind = if item.content_type == "files"
                && single_image_file_paths(item.file_paths.as_deref())
            {
                "image"
            } else {
                item.content_type.as_str()
            };
            let allowed = self.hover_preference.allows(content_kind);
            if !allowed {
                if self.hover_preview.id.is_some() {
                    self.close_hover_preview(cx);
                }
                return;
            }
            if self.hover_preview.id != Some(id) {
                self.close_hover_preview(cx);
                let generation = self.hover_preview.open(id);
                let delay = self.hover_preference.delay_ms;
                self.hover_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(u64::from(delay)))
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        if this.hover_preview.id == Some(id)
                            && this.hover_preview.generation == generation
                            && this.hover_source_active
                        {
                            this.send(Command::HoverPreview { id, generation }, cx);
                        }
                    });
                }));
            }
            self.hover_source_active = true;
            self.hover_close_task = None;
        } else if self.hover_preview.id == Some(id) {
            self.hover_source_active = false;
            self.schedule_hover_close(cx);
        }
    }

    pub(super) fn set_hover_popup_active(
        &mut self,
        id: i64,
        generation: u64,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        if self.hover_preview.id != Some(id) || self.hover_preview.generation != generation {
            return;
        }
        self.hover_popup_active = entered;
        if entered {
            self.hover_close_task = None;
        } else if !self.hover_source_active {
            self.schedule_hover_close(cx);
        }
    }

    pub(super) fn save_hover_preference(
        &mut self,
        preference: HoverPreviewPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.hover_preference_pending
            && self.hover_preference != preference
            && self.send(Command::SetHoverPreview(preference), cx)
        {
            self.hover_preference_pending = true;
            cx.notify();
        }
    }

    pub(super) fn open_hover_popup(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.hover_popup.is_some()
            || self.hover_popup_opening.is_some()
            || !self.hover_source_active
        {
            return;
        }
        let (Some(id), Some(result)) = (self.hover_preview.id, self.hover_preview.result.clone())
        else {
            return;
        };
        let generation = self.hover_preview.generation;
        let screen = window.display(cx).map(|display| display.bounds());
        // 浮窗一出现就开始固定 5 秒的存活计时（见 `HOVER_AUTOCLOSE`），不随鼠标活动续命。
        self.arm_hover_autoclose(cx);
        let image_dimensions =
            self.hover_preference
                .expanded_image
                .then(|| match &result {
                    Ok(PreviewContent::Image(_)) => self
                        .history
                        .items
                        .iter()
                        .find(|item| item.id == id)
                        .and_then(|item| Some((item.image_width?, item.image_height?))),
                    // 图片文件（截图工具放进剪贴板的 JPG/PNG）在库里没有尺寸列，读文件头即可。
                    Ok(PreviewContent::Files(entries)) => single_file_image_path(entries)
                        .and_then(|path| image_file_dimensions(&path)),
                    _ => None,
                })
                .flatten();
        let bounds = position::hover_popup_bounds(
            point(
                window.bounds().origin.x + window.mouse_position().x,
                window.bounds().origin.y + window.mouse_position().y,
            ),
            screen.unwrap_or_else(|| window.bounds()),
            self.hover_preference.position,
            image_dimensions,
        );
        let theme = self.theme;
        let language = self.language;
        let window_effect = self.window_effect;
        let zoom_step = self.hover_preference.zoom_step;
        self.hover_popup_opening = Some((id, generation));
        cx.spawn(async move |owner, cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let valid = owner_entity.update(cx, |this, _| {
                this.hover_preview.id == Some(id)
                    && this.hover_preview.generation == generation
                    && (this.hover_source_active || this.hover_popup_active)
            });
            if !valid {
                owner_entity.update(cx, |this, _| {
                    if this.hover_popup_opening == Some((id, generation)) {
                        this.hover_popup_opening = None;
                    }
                });
                return;
            }
            let popup_owner = owner.clone();
            // 刻意**不用** `gpui_kit::open_window`：它会用 `gpui_base::Root` 把 view 包一层，
            // 而 gpui-component 的 `WindowState` 根插件是**全局按类型注册**的，会在 `decorate`
            // 里给每个 Root 套上 `window_border()`。那个边框组件画一圈 1px 边框 + 一个
            // blur 10px / offset (0,+2px) 的 box shadow，而给它留空间的 `SHADOW_SIZE`
            // **只有 Linux 是 20px，Windows 上是 0** —— 阴影没有落脚处就溢进窗口内部，
            // 沿右/下边缘形成一条灰带（设置窗、预览窗都有，就是这个）。
            //
            // 悬停浮窗不需要 Root 提供的 dialog / notification / 文本选择层，
            // 直接走 GPUI 原生开窗即可绕开这个插件。已确认浮窗里用到的
            // `Button` / `Icon` 只依赖 GPUI 自身 API 和 App 级全局（`GlobalState`），不依赖 Root。
            let opened = cx.update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: None,
                        kind: WindowKind::PopUp,
                        focus: false,
                        show: true,
                        // 允许拖动：`WindowControlArea::Drag` 只在 `is_movable` 为真时才生效
                        // （见 GPUI Windows 后端的 hit-test）。预览浮窗常常盖住任务栏或别的
                        // 可点区域，用户得能把它挪开。
                        is_movable: true,
                        is_resizable: false,
                        app_id: Some("com.aslant.elegant-clipboard-gpui.hover".into()),
                        ..Default::default()
                    },
                    move |window, cx| {
                        apply_theme(theme, window, cx);
                        acrylic::init_window_effect(window_effect, window, cx);
                        cx.new(|cx| {
                            HoverPreviewWindowView::new(
                                popup_owner.clone(),
                                (id, generation),
                                language,
                                result,
                                zoom_step,
                                window,
                                cx,
                            )
                        })
                    },
                )
            });
            owner_entity.update(cx, |this, cx| {
                if this.hover_popup_opening == Some((id, generation)) {
                    this.hover_popup_opening = None;
                }
                if let Ok(handle) = opened {
                    // `cx.open_window` 返回的是带类型的 `WindowHandle`，这里统一成 `AnyWindowHandle`。
                    let handle = AnyWindowHandle::from(handle);
                    // 窗口此时已经创建完成（`open_window` 返回前至少画过一帧），再套一次窗口效果。
                    //
                    // 首次套用发生在 builder 闭包里，而那时 GPUI 正持有窗口借用：套用效果会让
                    // 客户区变大（去掉边框、加 DWM 背板与圆角），随后补发的 `WM_SIZE` 里
                    // 更新布局尺寸的回调会因为窗口被借用而静默失败，于是布局尺寸停在旧值，
                    // 右/下露出一条没被绘制的区域（背板透出来就是灰色带），要拖一下窗口才恢复。
                    // 这一次套用会把尺寸重新投递到消息队列，等本次更新结束后派发，回调才能生效。
                    let _ = handle.update(cx, |_, window, _| {
                        acrylic::apply(window_effect.effect, window);
                        window.refresh();
                    });
                    if this.hover_preview.id == Some(id)
                        && this.hover_preview.generation == generation
                        && (this.hover_source_active || this.hover_popup_active)
                    {
                        this.hover_popup = Some(handle);
                    } else {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();
    }
}
