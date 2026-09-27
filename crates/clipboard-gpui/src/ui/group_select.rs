use clipboard_core::{database::Group, preferences::LanguagePreference};
use gpui_kit::{SharedString, component::select::SelectItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GroupChoice {
    Default,
    Existing(i64),
    Create,
    Rename,
    Delete,
    MoveUp,
    MoveDown,
}

#[derive(Clone)]
pub(super) struct GroupOption {
    pub choice: GroupChoice,
    pub title: SharedString,
}

impl SelectItem for GroupOption {
    type Value = GroupChoice;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.choice
    }
}

pub(super) fn group_options(
    groups: &[Group],
    language: LanguagePreference,
    selected: Option<i64>,
) -> Vec<GroupOption> {
    let (default, create, rename, delete, move_up, move_down) = match language {
        LanguagePreference::Chinese => (
            "默认分组",
            "+ 新建分组",
            "重命名分组",
            "删除分组",
            "上移分组",
            "下移分组",
        ),
        LanguagePreference::English => (
            "Default group",
            "+ New group",
            "Rename group",
            "Delete group",
            "Move group up",
            "Move group down",
        ),
    };

    // Group snapshots are already ordered by the repository.
    let selected_index = selected.and_then(|id| groups.iter().position(|group| group.id == id));
    let mut options = Vec::with_capacity(groups.len() + 6);
    options.push(GroupOption {
        choice: GroupChoice::Default,
        title: default.into(),
    });
    options.extend(groups.iter().map(|group| GroupOption {
        choice: GroupChoice::Existing(group.id),
        title: group.name.clone().into(),
    }));
    options.push(GroupOption {
        choice: GroupChoice::Create,
        title: create.into(),
    });
    if let Some(index) = selected_index {
        options.push(GroupOption {
            choice: GroupChoice::Rename,
            title: rename.into(),
        });
        options.push(GroupOption {
            choice: GroupChoice::Delete,
            title: delete.into(),
        });
        if index > 0 {
            options.push(GroupOption {
                choice: GroupChoice::MoveUp,
                title: move_up.into(),
            });
        }
        if index + 1 < groups.len() {
            options.push(GroupOption {
                choice: GroupChoice::MoveDown,
                title: move_down.into(),
            });
        }
    }
    options
}
