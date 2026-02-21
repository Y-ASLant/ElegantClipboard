// 阻止 Windows release 版本弹出控制台窗口，请勿删除！！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    elegant_clipboard_lib::run()
}
