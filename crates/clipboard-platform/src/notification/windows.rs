use crate::instance::to_wide;
use anyhow::{Context, Result, anyhow, bail};
use std::{path::Path, sync::Mutex};
use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
    Win32::{
        Foundation::PROPERTYKEY,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READWRITE,
                StructuredStorage::PROPVARIANT,
            },
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        },
        UI::Shell::{IShellLinkW, PropertiesSystem::IPropertyStore, ShellLink},
    },
    core::{BSTR, GUID, HRESULT, HSTRING, Interface, PCWSTR},
};

const APP_ID: &str = "com.aslant.elegant-clipboard-gpui";
const APP_ID_KEY: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};
static REGISTERED: Mutex<bool> = Mutex::new(false);

struct Runtime;
impl Runtime {
    fn new() -> Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

fn register_shortcut() -> Result<()> {
    let base = directories::BaseDirs::new().context("无法确定系统通知注册目录")?;
    let programs = base
        .data_dir()
        .join("Microsoft/Windows/Start Menu/Programs/ElegantClipboard");
    let executable = std::env::current_exe()?;
    register_at(&programs.join("ElegantClipboard.lnk"), &executable)
}

fn register_at(shortcut: &Path, executable: &Path) -> Result<()> {
    let path = to_wide(&shortcut.to_string_lossy());
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let file: IPersistFile = link.cast()?;
    let properties: IPropertyStore = link.cast()?;
    if shortcut.exists() {
        // 只更新由本应用身份注册的快捷方式，不覆盖同名用户文件。
        unsafe { file.Load(PCWSTR(path.as_ptr()), STGM_READWRITE)? };
        let value = unsafe { properties.GetValue(&APP_ID_KEY)? };
        if BSTR::try_from(&value)? != APP_ID {
            bail!("同名开始菜单快捷方式不属于 ElegantClipboard 通知身份");
        }
    }
    let executable = to_wide(&executable.to_string_lossy());
    let value = PROPVARIANT::from(APP_ID);
    unsafe {
        link.SetPath(PCWSTR(executable.as_ptr()))?;
        link.SetDescription(PCWSTR(to_wide("ElegantClipboard").as_ptr()))?;
        link.SetIconLocation(PCWSTR(executable.as_ptr()), 0)?;
        properties.SetValue(&APP_ID_KEY, &value)?;
        properties.Commit()?;
    }
    let directory = shortcut.parent().context("通知快捷方式目录无效")?;
    std::fs::create_dir_all(directory)?;
    // 先保存临时文件，再替换已验证属于自己的快捷方式。
    let temporary = tempfile::Builder::new()
        .suffix(".lnk")
        .tempfile_in(directory)?
        .into_temp_path();
    let temporary_path = temporary.to_path_buf();
    unsafe {
        file.Save(
            PCWSTR(to_wide(&temporary_path.to_string_lossy()).as_ptr()),
            true,
        )?
    };
    drop(properties);
    drop(file);
    drop(link);
    temporary.persist(shortcut).map_err(|error| error.error)?;
    Ok(())
}

fn document(title: &str, message: &str) -> Result<XmlDocument> {
    let xml = XmlDocument::new()?;
    xml.LoadXml(&HSTRING::from(
        "<toast duration=\"short\"><visual><binding template=\"ToastGeneric\"><text/><text/></binding></visual><audio silent=\"true\"/></toast>",
    ))?;
    let text = xml.GetElementsByTagName(&HSTRING::from("text"))?;
    for (index, value) in [title, message].into_iter().enumerate() {
        // 使用文本节点，消息中的 XML 符号不会成为通知标记。
        text.Item(index as u32)?
            .AppendChild(&xml.CreateTextNode(&HSTRING::from(value))?)?;
    }
    Ok(xml)
}

pub(super) fn show(title: &str, message: &str) -> Result<()> {
    let _runtime = Runtime::new()?;
    let mut registered = REGISTERED
        .lock()
        .map_err(|_| anyhow!("系统通知注册锁不可用"))?;
    if !*registered {
        register_shortcut()?;
        *registered = true;
    }
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
        .context("创建 Windows 通知发送器失败")?;
    match notifier.Setting() {
        Ok(NotificationSetting::Enabled) => {}
        Ok(_) => bail!("系统已关闭 ElegantClipboard 通知"),
        // 新身份在首次发送前可能还没有系统设置项，仍应尝试 Show。
        Err(error) if error.code() == HRESULT::from_win32(1168) => {}
        Err(error) => return Err(error).context("查询 Windows 通知设置失败"),
    }
    let toast = ToastNotification::CreateToastNotification(&document(title, message)?)
        .context("创建 Windows 通知内容失败")?;
    toast
        .SetTag(&HSTRING::from("clipboard"))
        .context("设置通知标识失败")?;
    toast
        .SetGroup(&HSTRING::from("feedback"))
        .context("设置通知分组失败")?;
    notifier.Show(&toast).context("向 Windows 投递通知失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_shortcut_can_be_updated_without_overwriting_an_unowned_file() -> Result<()> {
        let _runtime = Runtime::new()?;
        let directory = tempfile::tempdir()?;
        let shortcut = directory.path().join("ElegantClipboard.lnk");
        let executable = std::env::current_exe()?;
        register_at(&shortcut, &executable)?;
        register_at(&shortcut, &executable)?;
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
        let file: IPersistFile = link.cast()?;
        unsafe {
            file.Load(
                PCWSTR(to_wide(&shortcut.to_string_lossy()).as_ptr()),
                windows::Win32::System::Com::STGM_READ,
            )?
        };
        let properties: IPropertyStore = link.cast()?;
        let value = unsafe { properties.GetValue(&APP_ID_KEY)? };
        assert_eq!(BSTR::try_from(&value)?, APP_ID);
        drop(file);
        drop(properties);
        drop(link);
        std::fs::write(&shortcut, b"unowned shortcut")?;
        assert!(register_at(&shortcut, &executable).is_err());
        assert_eq!(std::fs::read(&shortcut)?, b"unowned shortcut");
        Ok(())
    }

    #[test]
    fn message_markup_stays_text() -> Result<()> {
        let _runtime = Runtime::new()?;
        let title = "ElegantClipboard & <通知>";
        let message = "读取失败：\"内容\" <audio src='other'/>\n第二行";
        let xml = document(title, message)?;
        let nodes = xml.GetElementsByTagName(&HSTRING::from("text"))?;
        assert_eq!(nodes.Item(0)?.InnerText()?, HSTRING::from(title));
        assert_eq!(nodes.Item(1)?.InnerText()?, HSTRING::from(message));
        assert_eq!(
            xml.GetElementsByTagName(&HSTRING::from("audio"))?
                .Length()?,
            1
        );
        Ok(())
    }
}
