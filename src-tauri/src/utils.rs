/// 格式化字节数为人类可读的大小字符串（B/KB/MB）。
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn under_1kb() {
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn exactly_1kb() {
        assert_eq!(format_size(1024), "1.0 KB");
    }

    #[test]
    fn megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn fractional_kb() {
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
