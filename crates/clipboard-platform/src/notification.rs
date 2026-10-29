//! 各系统的原生桌面通知。由调用者放到后台执行，不创建应用内浮层。

#[cfg(target_os = "linux")]
#[path = "notification/linux.rs"]
mod native;
#[cfg(target_os = "macos")]
#[path = "notification/macos.rs"]
mod native;
#[cfg(windows)]
#[path = "notification/windows.rs"]
mod native;

/// 发送操作反馈；成功仅表示系统接受请求，实际展示由系统通知设置决定。
pub fn show(title: &str, message: &str) -> anyhow::Result<()> {
    native::show(title, message)
}
