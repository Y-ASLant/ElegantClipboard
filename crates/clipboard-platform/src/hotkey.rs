use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use std::{
    collections::HashSet,
    sync::mpsc,
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
            UnregisterHotKey,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG, PM_NOREMOVE,
            PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
        },
    },
};

const HOTKEY_ID: i32 = 1;

pub struct Hotkey {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl Hotkey {
    pub fn start(choice: HotkeyPreference, events: Sender<(isize, u32)>) -> Result<Self> {
        let (modifiers, key) = match choice {
            HotkeyPreference::CtrlShiftV => (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, b'V'),
            HotkeyPreference::AltC => (MOD_ALT | MOD_NOREPEAT, b'C'),
            HotkeyPreference::CtrlAltV => (MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'V'),
            HotkeyPreference::Disabled => return Err(anyhow!("快捷键已关闭")),
        };
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("history-hotkey".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                let registration =
                    unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers, key as u32) };
                let registered = registration.is_ok();
                let _ = ready.send(registration.map(|_| thread_id));
                if !registered {
                    return;
                }
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                    if message.message == WM_HOTKEY && message.wParam.0 == HOTKEY_ID as usize {
                        let foreground = unsafe { GetForegroundWindow() };
                        let mut process_id = 0;
                        unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
                        let _ = events.try_send((foreground.0 as isize, process_id));
                    }
                }
                unsafe {
                    let _ = UnregisterHotKey(None, HOTKEY_ID);
                }
            })?;
        match receiver.recv().context("快捷键线程未响应")? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(anyhow!("{} 注册失败：{error}", choice.label()))
            }
        }
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteHotkeyEvent {
    pub slot: u8,
    pub favorite: bool,
    pub target: (isize, u32),
}

pub struct PasteHotkeys {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug)]
pub struct PasteRegistrationWarning {
    pub slot: u8,
    pub favorite: bool,
    pub message: String,
}

impl std::fmt::Display for PasteRegistrationWarning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

struct ParsedPasteShortcut {
    canonical: String,
    modifiers: HOT_KEY_MODIFIERS,
    key: u32,
    numpad_key: Option<u32>,
}

fn parse_paste_shortcut(value: &str) -> Result<ParsedPasteShortcut> {
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        return Err(anyhow!("快捷键需要修饰键和主键"));
    }
    let mut control = false;
    let mut alt = false;
    let mut shift = false;
    for modifier in &parts[..parts.len() - 1] {
        let used = match modifier.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => &mut control,
            "ALT" => &mut alt,
            "SHIFT" => &mut shift,
            _ => return Err(anyhow!("不支持的快捷键修饰键：{modifier}")),
        };
        if *used {
            return Err(anyhow!("快捷键修饰键重复：{modifier}"));
        }
        *used = true;
    }
    if !control && !alt {
        return Err(anyhow!("快速粘贴快捷键至少需要 Ctrl 或 Alt"));
    }
    let key_name = parts[parts.len() - 1].to_ascii_uppercase();
    let (key, numpad_key, key_label) =
        if key_name.len() == 1 && key_name.as_bytes()[0].is_ascii_alphanumeric() {
            let key = u32::from(key_name.as_bytes()[0]);
            let numpad = key_name.as_bytes()[0]
                .is_ascii_digit()
                .then(|| 0x60 + u32::from(key_name.as_bytes()[0] - b'0'));
            (key, numpad, key_name)
        } else if let Some(digit) = key_name
            .strip_prefix("NUMPAD")
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|digit| *digit <= 9)
        {
            (0x60 + u32::from(digit), None, format!("Numpad{digit}"))
        } else if let Some(function) = key_name
            .strip_prefix('F')
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|function| (1..=24).contains(function))
        {
            (0x6f + u32::from(function), None, format!("F{function}"))
        } else {
            let key = match key_name.as_str() {
                "SPACE" => 0x20,
                "TAB" => 0x09,
                "ENTER" => 0x0d,
                "ESC" | "ESCAPE" => 0x1b,
                "BACKSPACE" => 0x08,
                "DELETE" => 0x2e,
                "INSERT" => 0x2d,
                "HOME" => 0x24,
                "END" => 0x23,
                "PAGEUP" => 0x21,
                "PAGEDOWN" => 0x22,
                "LEFT" => 0x25,
                "UP" => 0x26,
                "RIGHT" => 0x27,
                "DOWN" => 0x28,
                _ => return Err(anyhow!("不支持的快捷键主键：{}", parts[parts.len() - 1])),
            };
            (key, None, key_name)
        };
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut canonical = Vec::new();
    if control {
        modifiers |= MOD_CONTROL;
        canonical.push("Ctrl");
    }
    if alt {
        modifiers |= MOD_ALT;
        canonical.push("Alt");
    }
    if shift {
        modifiers |= MOD_SHIFT;
        canonical.push("Shift");
    }
    canonical.push(&key_label);
    Ok(ParsedPasteShortcut {
        canonical: canonical.join("+"),
        modifiers,
        key,
        numpad_key,
    })
}

pub fn normalize_paste_shortcut(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(parse_paste_shortcut(value)?.canonical)
}

pub fn validate_paste_shortcuts(
    shortcuts: &PasteShortcutConfig,
    main_hotkey: HotkeyPreference,
) -> Result<()> {
    let main = match main_hotkey {
        HotkeyPreference::CtrlShiftV => Some(((MOD_CONTROL | MOD_SHIFT).0, u32::from(b'V'))),
        HotkeyPreference::AltC => Some((MOD_ALT.0, u32::from(b'C'))),
        HotkeyPreference::CtrlAltV => Some(((MOD_CONTROL | MOD_ALT).0, u32::from(b'V'))),
        HotkeyPreference::Disabled => None,
    };
    let mut used = HashSet::new();
    for favorite in [false, true] {
        for slot in 1..=10u8 {
            let value = shortcuts.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let parsed =
                parse_paste_shortcut(value).with_context(|| format!("槽位 {slot} 的快捷键无效"))?;
            let primary = (parsed.modifiers.0, parsed.key);
            if Some(primary) == main {
                return Err(anyhow!("{} 与窗口唤出快捷键冲突", parsed.canonical));
            }
            if !used.insert(primary) {
                return Err(anyhow!("{} 被多个快速粘贴槽位使用", parsed.canonical));
            }
            if let Some(numpad_key) = parsed.numpad_key
                && !used.insert((parsed.modifiers.0, numpad_key))
            {
                return Err(anyhow!(
                    "{} 的数字小键盘变体与其他槽位冲突",
                    parsed.canonical
                ));
            }
        }
    }
    Ok(())
}

impl PasteHotkeys {
    /// Registers configured slots, including a number-pad variant for digit keys.
    pub fn start(
        events: Sender<PasteHotkeyEvent>,
        shortcuts: &PasteShortcutConfig,
        main_hotkey: HotkeyPreference,
    ) -> Result<(Self, Vec<PasteRegistrationWarning>)> {
        validate_paste_shortcuts(shortcuts, main_hotkey)?;
        let shortcuts = shortcuts.clone();
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("paste-hotkeys".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                let mut registered = Vec::new();
                let mut failures = Vec::new();
                for favorite in [false, true] {
                    for slot in 1..=10u8 {
                        let shortcut = shortcuts.slot(favorite, slot).unwrap_or_default();
                        if shortcut.is_empty() {
                            continue;
                        }
                        let parsed = parse_paste_shortcut(shortcut).expect("validated shortcut");
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
                let registered_any = !registered.is_empty();
                let _ = ready.send((thread_id, failures, registered_any));
                if registered_any {
                    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                        if message.message == WM_HOTKEY
                            && let Some((slot, favorite)) =
                                paste_slot_for_id(message.wParam.0 as i32)
                        {
                            let foreground = unsafe { GetForegroundWindow() };
                            let mut process_id = 0;
                            unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
                            let _ = events.try_send(PasteHotkeyEvent {
                                slot,
                                favorite,
                                target: (foreground.0 as isize, process_id),
                            });
                        }
                    }
                }
                for id in registered {
                    unsafe {
                        let _ = UnregisterHotKey(None, id);
                    }
                }
            })?;
        let (thread_id, failures, registered_any) =
            receiver.recv().context("快速粘贴快捷键线程未响应")?;
        if !registered_any && !failures.is_empty() {
            let _ = worker.join();
            return Err(anyhow!(
                "快速粘贴快捷键均被其他程序占用：{}",
                failures
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        Ok((
            Self {
                thread_id,
                worker: Some(worker),
            },
            failures,
        ))
    }
}

impl Drop for PasteHotkeys {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
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
    use super::{normalize_paste_shortcut, paste_slot_for_id, validate_paste_shortcuts};
    use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};

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
        assert!(validate_paste_shortcuts(&config, HotkeyPreference::CtrlShiftV).is_ok());
        config.set_slot(true, 4, "Alt+Numpad1".into()).unwrap();
        assert!(validate_paste_shortcuts(&config, HotkeyPreference::CtrlShiftV).is_err());
        config.set_slot(true, 4, "Ctrl+Shift+V".into()).unwrap();
        assert!(validate_paste_shortcuts(&config, HotkeyPreference::CtrlShiftV).is_err());
    }
}
