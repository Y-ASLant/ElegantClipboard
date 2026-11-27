use super::*;

/// Retains only the list render boundary; all domain and interaction state stays
/// on ClipboardView. Rendering happens after the parent's borrow has ended.
pub(super) struct ListSurface {
    owner: WeakEntity<ClipboardView>,
    _owner_subscription: Subscription,
}

impl ListSurface {
    pub fn new(owner: &Entity<ClipboardView>, cx: &mut Context<Self>) -> Self {
        Self {
            owner: owner.downgrade(),
            _owner_subscription: cx.observe(owner, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for ListSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self
            .owner
            .update(cx, |view, cx| {
                if view.todos_open {
                    view.sync_todo_layout(window, cx);
                    let request = if view.card_drag_active(cx) {
                        view.todo_drag_source_id().zip(view.todo_drop_target)
                    } else if view.todo_reorder_pending {
                        view.drag_motion.request
                    } else {
                        None
                    };
                    let layout = view.todo_layout.borrow();
                    view.drag_motion.update(
                        request,
                        |id| layout.ids.iter().position(|&row| row == id),
                        &layout.offsets,
                    );
                    drop(layout);
                    view.render_todos(cx).into_any_element()
                } else {
                    let request = if view.card_drag_active(cx) {
                        view.drag_source_id().zip(view.drop_target)
                    } else if view.reorder_pending {
                        view.drag_motion.request
                    } else {
                        None
                    };
                    view.drag_motion.update(
                        request,
                        |id| view.history.item_index(id),
                        &view.row_offsets,
                    );
                    view.render_history_list(history_empty_state(view, cx), cx)
                        .into_any_element()
                }
            })
            .unwrap_or_else(|_| div().into_any_element());
        div().flex().flex_col().size_full().child(content)
    }
}

/// Caches measured search/toolbar presentation independently of list animation.
pub(super) struct ListChrome {
    owner: WeakEntity<ClipboardView>,
    pub height: Rc<Cell<Option<Pixels>>>,
    _owner_subscription: Subscription,
}

impl ListChrome {
    pub fn new(owner: &Entity<ClipboardView>, cx: &mut Context<Self>) -> Self {
        Self {
            owner: owner.downgrade(),
            height: Rc::new(Cell::new(None)),
            _owner_subscription: cx.observe(owner, |this, _, cx| {
                this.height.set(None);
                cx.notify();
            }),
        }
    }
}

impl Render for ListChrome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self
            .owner
            .update(cx, |view, cx| view.render_list_chrome(window, cx))
            .unwrap_or_else(|_| div().into_any_element());
        let height = self.height.clone();
        let entity = cx.entity_id();
        div().relative().w_full().flex_none().child(content).child(
            canvas(
                move |bounds, window, _| {
                    if height.replace(Some(bounds.size.height)) != Some(bounds.size.height) {
                        window.on_next_frame(move |_, cx| cx.notify(entity));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }
}

fn history_empty_state(
    view: &ClipboardView,
    cx: &App,
) -> (gpui_kit::assets::IconName, &'static str) {
    use gpui_kit::assets::IconName;

    let language = view.language;
    let (icon, zh, en) = if view.history.loading {
        (IconName::ClipboardList, "正在加载", "Loading")
    } else if view.history_reply_missed {
        (
            IconName::CircleX,
            "历史列表没有返回，可重试",
            "History did not load. You can retry.",
        )
    } else if !view.search.read(cx).value().is_empty() {
        (
            IconName::SearchX,
            "没有匹配的记录，试试其他关键词",
            "No matching items. Try another search.",
        )
    } else if view.history.favorite_only() {
        (
            IconName::Heart,
            "还没有收藏，点击记录上的“收藏”保留常用文本",
            "No favorites yet. Mark an item as a favorite to keep it handy.",
        )
    } else if view.history.group_id().is_some() {
        (
            IconName::FolderOpen,
            "该分组暂无可显示的记录",
            "There are no items in this group.",
        )
    } else {
        match view.history.category() {
            ContentCategory::Text => (IconName::Type, "还没有文本记录", "No text items yet."),
            ContentCategory::Image => (IconName::Image, "还没有图片记录", "No image items yet."),
            ContentCategory::Url => (IconName::Link, "还没有链接记录", "No link items yet."),
            ContentCategory::Other => (IconName::File, "还没有其他类型记录", "No other items yet."),
            _ => (
                IconName::Clipboard,
                "复制一段文本，它会出现在这里",
                "Copy something and it will appear here.",
            ),
        }
    };
    (icon, tr(language, zh, en))
}
