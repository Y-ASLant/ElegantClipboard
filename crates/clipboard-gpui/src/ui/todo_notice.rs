use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TodoNotice {
    Added,
    StatusSet { id: i64, status: TodoStatus },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoticeStatus {
    Show,
    Wait,
    Drop,
}

pub(super) fn notice_status(notice: TodoNotice, before: &[Todo], after: &[Todo]) -> NoticeStatus {
    match notice {
        TodoNotice::Added => {
            if after
                .iter()
                .any(|todo| before.iter().all(|old| old.id != todo.id))
            {
                NoticeStatus::Show
            } else {
                NoticeStatus::Wait
            }
        }
        TodoNotice::StatusSet { id, status } => {
            let was_other = before
                .iter()
                .any(|todo| todo.id == id && todo.status != status);
            let matched = after
                .iter()
                .any(|todo| todo.id == id && todo.status == status);
            if was_other && matched {
                NoticeStatus::Show
            } else if was_other {
                NoticeStatus::Wait
            } else {
                NoticeStatus::Drop
            }
        }
    }
}

pub(super) fn notice_text(
    notice: TodoNotice,
    before: &[Todo],
    after: &[Todo],
    language: LanguagePreference,
) -> String {
    let text = match notice {
        TodoNotice::Added => after
            .iter()
            .find(|todo| before.iter().all(|old| old.id != todo.id))
            .map(|todo| todo.text.as_str())
            .unwrap_or(""),
        TodoNotice::StatusSet { id, .. } => after
            .iter()
            .find(|todo| todo.id == id)
            .map(|todo| todo.text.as_str())
            .unwrap_or(""),
    };
    let excerpt = super::todo_card::card_excerpt(text);
    let excerpt = truncate_excerpt(&excerpt, 40);
    match notice {
        TodoNotice::Added => match (language, excerpt.is_empty()) {
            (LanguagePreference::Chinese, true) => "已添加待办".to_owned(),
            (LanguagePreference::Chinese, false) => format!("已添加：{excerpt}"),
            (_, true) => "To-do added".to_owned(),
            (_, false) => format!("Added: {excerpt}"),
        },
        TodoNotice::StatusSet { status, .. } => {
            let verb = match (language, status) {
                (LanguagePreference::Chinese, TodoStatus::Done) => "已处理",
                (LanguagePreference::Chinese, TodoStatus::Closed) => "已关闭",
                (_, TodoStatus::Done) => "Done",
                (_, TodoStatus::Closed) => "Closed",
                (LanguagePreference::Chinese, _) => "已更新",
                (_, _) => "Updated",
            };
            if excerpt.is_empty() {
                match language {
                    LanguagePreference::Chinese => format!("{verb}待办"),
                    LanguagePreference::English => format!("To-do {verb}"),
                }
            } else {
                match language {
                    LanguagePreference::Chinese => format!("{verb}：{excerpt}"),
                    LanguagePreference::English => format!("{verb}: {excerpt}"),
                }
            }
        }
    }
}

pub(super) fn todo_status_label(language: LanguagePreference, status: TodoStatus) -> &'static str {
    match (language, status) {
        (LanguagePreference::Chinese, TodoStatus::Pending) => "待处理",
        (LanguagePreference::Chinese, TodoStatus::InProgress) => "处理中",
        (LanguagePreference::Chinese, TodoStatus::Done) => "已处理",
        (LanguagePreference::Chinese, TodoStatus::Closed) => "已关闭",
        (_, TodoStatus::Pending) => "Pending",
        (_, TodoStatus::InProgress) => "In progress",
        (_, TodoStatus::Done) => "Done",
        (_, TodoStatus::Closed) => "Closed",
    }
}

fn truncate_excerpt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

pub(super) fn show_todo_notice(
    view: &mut ClipboardView,
    message: String,
    window: &mut Window,
    cx: &mut Context<ClipboardView>,
) {
    if let Some(handle) = view.todo_toast
        && handle
            .update(cx, |toast, window, cx| {
                toast.replace(message.clone(), window, cx)
            })
            .is_ok()
    {
        return;
    }
    view.todo_toast = super::todo_toast::open(
        message,
        tr(view.language, "关闭", "Close"),
        view.theme,
        cx.weak_entity(),
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::{NoticeStatus, TodoNotice, notice_status, notice_text};
    use clipboard_core::preferences::LanguagePreference;
    use clipboard_core::{Todo, TodoStatus};

    fn todo(id: i64, text: &str, status: TodoStatus) -> Todo {
        Todo {
            id,
            text: text.to_owned(),
            status,
            created_at: "2026-11-10 09:00:00".into(),
            updated_at: "2026-11-10 09:00:00".into(),
        }
    }

    #[test]
    fn notice_waits_for_the_matching_list_and_names_the_change() {
        let before = vec![todo(1, "buy milk", TodoStatus::Pending)];
        assert_eq!(
            notice_status(TodoNotice::Added, &before, &before),
            NoticeStatus::Wait
        );
        let added = vec![
            todo(2, "**write** notes", TodoStatus::Pending),
            todo(1, "buy milk", TodoStatus::Pending),
        ];
        assert_eq!(
            notice_status(TodoNotice::Added, &before, &added),
            NoticeStatus::Show
        );
        assert_eq!(
            notice_text(
                TodoNotice::Added,
                &before,
                &added,
                LanguagePreference::Chinese
            ),
            "已添加：write notes"
        );
        let notice = TodoNotice::StatusSet {
            id: 1,
            status: TodoStatus::Done,
        };
        assert_eq!(notice_status(notice, &before, &before), NoticeStatus::Wait);
        let done = vec![todo(1, "buy milk", TodoStatus::Done)];
        assert_eq!(notice_status(notice, &before, &done), NoticeStatus::Show);
        assert_eq!(
            notice_text(notice, &before, &done, LanguagePreference::English),
            "Done: buy milk"
        );
        assert_eq!(
            notice_status(
                TodoNotice::StatusSet {
                    id: 9,
                    status: TodoStatus::Done
                },
                &before,
                &done
            ),
            NoticeStatus::Drop
        );
    }
}
