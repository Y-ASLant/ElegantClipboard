use anyhow::{Context, Result, bail};
use gtk::{gio, glib::variant::ToVariant};
use x11rb::protocol::xproto::ConnectionExt;

pub fn initialize() -> Result<()> {
    gtk::init().context("无法初始化系统托盘 GTK 事件循环")?;
    if status_notifier_host() || xembed_host() {
        return Ok(());
    }
    bail!(
        "系统未提供托盘宿主；窗口将保持可见。GNOME 可安装 AppIndicator 扩展。 / No tray host is available; the window stays visible. GNOME requires an AppIndicator extension."
    )
}

fn status_notifier_host() -> bool {
    let Ok(connection) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        return false;
    };
    let parameters = (
        "org.kde.StatusNotifierWatcher",
        "IsStatusNotifierHostRegistered",
    )
        .to_variant();
    connection
        .call_sync(
            Some("org.kde.StatusNotifierWatcher"),
            "/StatusNotifierWatcher",
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|reply| reply.get::<(gtk::glib::Variant,)>())
        .and_then(|(value,)| value.get::<bool>())
        .unwrap_or(false)
}

fn xembed_host() -> bool {
    let Ok((connection, screen)) = x11rb::connect(None) else {
        return false;
    };
    let name = format!("_NET_SYSTEM_TRAY_S{screen}");
    let Ok(atom) = connection.intern_atom(true, name.as_bytes()) else {
        return false;
    };
    let Ok(atom) = atom.reply() else {
        return false;
    };
    if atom.atom == x11rb::NONE {
        return false;
    }
    connection
        .get_selection_owner(atom.atom)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.owner != x11rb::NONE)
}
