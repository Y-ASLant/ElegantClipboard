use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG, PM_NOREMOVE,
            PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
        },
    },
};

const HOTKEY_ID: i32 = 1;

pub use super::shortcut_parse::{
    normalize_main_shortcut, normalize_paste_shortcut, validate_paste_shortcuts,
    validate_sequence_shortcut,
};
use super::shortcut_parse::{parse_paste_shortcut, parse_shortcut};

pub(super) fn main_hotkey_binding(
    choice: &HotkeyPreference,
) -> Result<Option<(HOT_KEY_MODIFIERS, u32)>> {
    match choice {
        HotkeyPreference::Shortcut(value) => {
            let parsed = parse_shortcut(value, true)?;
            Ok(Some((parsed.modifiers, parsed.key)))
        }
        HotkeyPreference::Disabled => Ok(None),
    }
}

fn foreground_target() -> (isize, u32) {
    let foreground = unsafe { GetForegroundWindow() };
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
    (foreground.0 as isize, process_id)
}

/// 主热键与快速粘贴槽位线程共用的消息循环脚手架：线程命名、消息队列
/// 预注入、线程号握手与退出收尾都集中在这里，调用方只保留各自的注册与
/// 分发逻辑。
struct MessageThread {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl MessageThread {
    /// 在专用线程上运行 `run` 并阻塞等待握手回报，`run` 闭包负责先通过
    /// `ready` 通道发送 `(线程号, 结果)` 再进入消息处理。
    fn spawn<R: Send + 'static>(
        name: &'static str,
        handshake_context: &'static str,
        run: impl FnOnce(u32, &mut MSG, mpsc::SyncSender<(u32, R)>) + Send + 'static,
    ) -> Result<(Self, R)> {
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new().name(name.into()).spawn(move || {
            let mut message = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            }
            let thread_id = unsafe { GetCurrentThreadId() };
            run(thread_id, &mut message, ready);
        })?;
        let (thread_id, payload) = receiver.recv().context(handshake_context)?;
        Ok((
            Self {
                thread_id,
                worker: Some(worker),
            },
            payload,
        ))
    }

    /// 注册全部失败、线程已自行退出时仅回收线程，不投递退出消息。
    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn stop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        self.join_worker();
    }
}

/// 以 `dispatch` 分发每条消息，泵消息队列直到 GetMessageW 返回非正值。
fn pump_messages(message: &mut MSG, mut dispatch: impl FnMut(&MSG)) {
    while unsafe { GetMessageW(message, None, 0, 0) }.0 > 0 {
        dispatch(message);
    }
}

pub struct Hotkey {
    thread: MessageThread,
}

impl Hotkey {
    pub fn start(choice: &HotkeyPreference, events: Sender<(isize, u32)>) -> Result<Self> {
        let (modifiers, key) =
            main_hotkey_binding(choice)?.ok_or_else(|| anyhow!("快捷键已关闭"))?;
        let (mut thread, registration) = MessageThread::spawn(
            "history-hotkey",
            "快捷键线程未响应",
            move |thread_id, message, ready| {
                let registration =
                    unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers | MOD_NOREPEAT, key) };
                let registered = registration.is_ok();
                let _ = ready.send((thread_id, registration));
                if !registered {
                    return;
                }
                pump_messages(message, |message| {
                    if message.message == WM_HOTKEY && message.wParam.0 == HOTKEY_ID as usize {
                        let _ = events.try_send(foreground_target());
                    }
                });
                unsafe {
                    let _ = UnregisterHotKey(None, HOTKEY_ID);
                }
            },
        )?;
        match registration {
            Ok(()) => Ok(Self { thread }),
            Err(error) => {
                thread.join_worker();
                Err(anyhow!("{} 注册失败：{error}", choice.label()))
            }
        }
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        self.thread.stop();
    }
}

pub use crate::paste_types::{PasteHotkeyEvent, PasteRegistrationWarning, SequenceAdvanceEvent};

/// 序列粘贴推进键的注册 ID（与快速粘贴槽位同一线程）。
const SEQUENCE_ADVANCE_ID: i32 = 300;

pub struct PasteHotkeys {
    thread: MessageThread,
}

impl PasteHotkeys {
    /// Registers configured slots, including a number-pad variant for digit keys.
    /// `sequence_shortcut` 是序列粘贴推进键（决策 0004）；空串表示停用。
    /// 推进键常驻注册（未激活序列时事件由 UI 忽略）；`slots_enabled` 控制槽位是否注册。
    pub fn start(
        events: Sender<PasteHotkeyEvent>,
        sequence_events: Sender<SequenceAdvanceEvent>,
        shortcuts: &PasteShortcutConfig,
        sequence_shortcut: &str,
        slots_enabled: bool,
        main_hotkey: &HotkeyPreference,
    ) -> Result<(Self, Vec<PasteRegistrationWarning>)> {
        validate_paste_shortcuts(shortcuts, main_hotkey)?;
        validate_sequence_shortcut(shortcuts, sequence_shortcut, main_hotkey)?;
        let shortcuts = shortcuts.clone();
        let sequence_shortcut = sequence_shortcut.to_owned();
        let (mut thread, (failures, registered_any)) = MessageThread::spawn(
            "paste-hotkeys",
            "快速粘贴快捷键线程未响应",
            move |thread_id, message, ready| {
                let mut registered = Vec::new();
                let mut failures = Vec::new();
                let mut sequence_id_registered = false;
                if !sequence_shortcut.trim().is_empty() {
                    let parsed = parse_paste_shortcut(&sequence_shortcut)
                        .expect("validated sequence shortcut");
                    match unsafe {
                        RegisterHotKey(
                            None,
                            SEQUENCE_ADVANCE_ID,
                            parsed.modifiers | MOD_NOREPEAT,
                            parsed.key,
                        )
                    } {
                        Ok(()) => {
                            registered.push(SEQUENCE_ADVANCE_ID);
                            sequence_id_registered = true;
                        }
                        Err(error) => failures.push(PasteRegistrationWarning {
                            slot: 0,
                            favorite: false,
                            message: format!(
                                "{sequence_shortcut} ({SEQUENCE_ADVANCE_ID}): {error}"
                            ),
                        }),
                    }
                }
                if slots_enabled {
                    for favorite in [false, true] {
                        for slot in 1..=10u8 {
                            let shortcut = shortcuts.slot(favorite, slot).unwrap_or_default();
                            if shortcut.is_empty() {
                                continue;
                            }
                            let parsed =
                                parse_paste_shortcut(shortcut).expect("validated shortcut");
                            let id_base = if favorite { 200 } else { 100 };
                            for (offset, key) in [
                                (0, parsed.key),
                                (10, parsed.numpad_key.unwrap_or(parsed.key)),
                            ] {
                                if offset == 10 && parsed.numpad_key.is_none() {
                                    continue;
                                }
                                let id = id_base + offset + i32::from(slot);
                                match unsafe {
                                    RegisterHotKey(None, id, parsed.modifiers | MOD_NOREPEAT, key)
                                } {
                                    Ok(()) => registered.push(id),
                                    Err(error) => failures.push(PasteRegistrationWarning {
                                        slot,
                                        favorite,
                                        message: format!("{shortcut} ({id}): {error}"),
                                    }),
                                }
                            }
                        }
                    }
                }
                let registered_any = !registered.is_empty();
                let _ = ready.send((thread_id, (failures, registered_any)));
                if registered_any {
                    pump_messages(message, |message| {
                        if message.message != WM_HOTKEY {
                            return;
                        }
                        if message.wParam.0 as i32 == SEQUENCE_ADVANCE_ID {
                            if sequence_id_registered {
                                let _ = sequence_events.try_send(SequenceAdvanceEvent {
                                    target: foreground_target(),
                                });
                            }
                            return;
                        }
                        if let Some((slot, favorite)) = paste_slot_for_id(message.wParam.0 as i32) {
                            let _ = events.try_send(PasteHotkeyEvent {
                                slot,
                                favorite,
                                target: foreground_target(),
                            });
                        }
                    });
                }
                for id in registered {
                    unsafe {
                        let _ = UnregisterHotKey(None, id);
                    }
                }
            },
        )?;
        if !registered_any && !failures.is_empty() {
            thread.join_worker();
            return Err(anyhow!(
                "全部 {} 个快速粘贴组合键无法注册（如 {}）；请在设置 → 快捷键中更换组合键或关闭快速粘贴",
                failures.len(),
                failures[0],
            ));
        }
        Ok((Self { thread }, failures))
    }
}

impl Drop for PasteHotkeys {
    fn drop(&mut self) {
        self.thread.stop();
    }
}

fn paste_slot_for_id(id: i32) -> Option<(u8, bool)> {
    match id {
        101..=110 => Some(((id - 100) as u8, false)),
        111..=120 => Some(((id - 110) as u8, false)),
        201..=210 => Some(((id - 200) as u8, true)),
        211..=220 => Some(((id - 210) as u8, true)),
        _ => None,
    }
}

#[cfg(test)]
mod paste_tests {
    use super::{
        normalize_main_shortcut, normalize_paste_shortcut, paste_slot_for_id,
        validate_paste_shortcuts,
    };
    use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};

    #[test]
    fn main_shortcuts_validate_modifiers_keys_and_numpad_alias_conflicts() {
        assert_eq!(
            normalize_main_shortcut(" windows + control + f9 ").unwrap(),
            "Ctrl+Win+F9"
        );
        for invalid in [
            "",
            "Ctrl+",
            "Ctrl+Ctrl+V",
            "Shift+A",
            "Win+F25",
            "Alt+Numpad10",
        ] {
            assert!(normalize_main_shortcut(invalid).is_err(), "{invalid}");
        }
        let config = PasteShortcutConfig::default();
        for shortcut in ["Alt+1", "Alt+Numpad1"] {
            assert!(
                validate_paste_shortcuts(&config, &HotkeyPreference::Shortcut(shortcut.into()))
                    .is_err()
            );
        }
        assert!(
            validate_paste_shortcuts(&config, &HotkeyPreference::Shortcut("Ctrl+Win+F9".into()))
                .is_ok()
        );
    }

    #[test]
    fn paste_ids_map_number_row_and_numpad_to_same_slots() {
        assert_eq!(paste_slot_for_id(101), Some((1, false)));
        assert_eq!(paste_slot_for_id(120), Some((10, false)));
        assert_eq!(paste_slot_for_id(203), Some((3, true)));
        assert_eq!(paste_slot_for_id(211), Some((1, true)));
        assert_eq!(paste_slot_for_id(220), Some((10, true)));
        assert_eq!(paste_slot_for_id(221), None);
    }

    #[test]
    fn paste_shortcuts_normalize_and_reject_collisions() {
        assert_eq!(
            normalize_paste_shortcut(" alt + shift + f12 ").unwrap(),
            "Alt+Shift+F12"
        );
        assert_eq!(
            normalize_paste_shortcut("Ctrl+Numpad3").unwrap(),
            "Ctrl+Numpad3"
        );
        assert_eq!(normalize_paste_shortcut("  ").unwrap(), "");
        assert!(normalize_paste_shortcut("Shift+1").is_err());
        assert!(normalize_paste_shortcut("Win+1").is_err());
        let mut config = PasteShortcutConfig::default();
        assert!(validate_paste_shortcuts(&config, &HotkeyPreference::default()).is_ok());
        config.set_slot(true, 4, "Alt+Numpad1".into()).unwrap();
        assert!(validate_paste_shortcuts(&config, &HotkeyPreference::default()).is_err());
        config
            .set_slot(true, 4, HotkeyPreference::default().label().into())
            .unwrap();
        assert!(validate_paste_shortcuts(&config, &HotkeyPreference::default()).is_err());
    }
}
