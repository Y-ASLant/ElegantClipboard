/// 从 Windows 注册表读取系统代理设置
#[cfg(target_os = "windows")]
pub fn get_windows_system_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;

    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return None;
    }

    let server: String = key.get_value("ProxyServer").ok()?;
    let server = server.trim().to_string();
    if server.is_empty() {
        return None;
    }

    // 处理格式：可能是 "127.0.0.1:7890" 或 "http=...;https=...;socks=..."
    if server.contains('=') {
        // 多协议格式，优先取 https，其次 http，再 socks
        for part in server.split(';') {
            let part = part.trim();
            if let Some(addr) = part.strip_prefix("https=") {
                let addr = addr.trim();
                if !addr.is_empty() {
                    return Some(format_proxy_url(addr));
                }
            }
        }
        for part in server.split(';') {
            let part = part.trim();
            if let Some(addr) = part.strip_prefix("http=") {
                let addr = addr.trim();
                if !addr.is_empty() {
                    return Some(format_proxy_url(addr));
                }
            }
        }
        for part in server.split(';') {
            let part = part.trim();
            if let Some(addr) = part.strip_prefix("socks=") {
                let addr = addr.trim();
                if !addr.is_empty() {
                    return Some(format!("socks5://{}", addr));
                }
            }
        }
        None
    } else {
        Some(format_proxy_url(&server))
    }
}

#[cfg(target_os = "windows")]
fn format_proxy_url(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") || addr.starts_with("socks5://") || addr.starts_with("socks4://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_windows_system_proxy() -> Option<String> {
    None
}

/// 根据代理模式配置 reqwest 客户端 builder
pub fn apply_proxy(
    mut builder: reqwest::blocking::ClientBuilder,
    proxy_mode: &str,
    proxy_url: &str,
) -> Result<reqwest::blocking::ClientBuilder, String> {
    match proxy_mode {
        "none" => {
            builder = builder.no_proxy();
        }
        "custom" => {
            let url = proxy_url.trim();
            if !url.is_empty() {
                let proxy = reqwest::Proxy::all(url)
                    .map_err(|e| format!("代理配置无效: {}", e))?;
                builder = builder.proxy(proxy);
            } else {
                builder = builder.no_proxy();
            }
        }
        _ => {
            // "system"：优先从 Windows 注册表读取，回退到 reqwest 默认（环境变量）
            if let Some(sys_proxy) = get_windows_system_proxy() {
                tracing::info!("使用 Windows 系统代理: {}", sys_proxy);
                let proxy = reqwest::Proxy::all(&sys_proxy)
                    .map_err(|e| format!("系统代理配置无效: {}", e))?;
                builder = builder.proxy(proxy);
            }
            // 否则 reqwest 默认行为（读取 HTTP_PROXY 等环境变量）
        }
    }
    Ok(builder)
}
