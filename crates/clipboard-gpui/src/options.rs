use anyhow::{Result, bail};
use std::{ffi::OsString, path::PathBuf};

pub struct Options {
    pub data_dir: Option<PathBuf>,
    pub monitor: bool,
    pub smoke_test: bool,
    pub import_db: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>> {
        let mut options = Self {
            data_dir: None,
            monitor: true,
            smoke_test: false,
            import_db: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => return Ok(None),
                Some("--data-dir") => {
                    let Some(path) = args.next().filter(|path| {
                        !path.is_empty() && !path.to_string_lossy().starts_with("--")
                    }) else {
                        bail!("--data-dir 需要目录路径");
                    };
                    options.data_dir = Some(path.into());
                }
                Some("--no-monitor") => options.monitor = false,
                Some("--smoke-test") => options.smoke_test = true,
                Some("--import-db") => {
                    let Some(path) = args.next().filter(|path| {
                        !path.is_empty() && !path.to_string_lossy().starts_with("--")
                    }) else {
                        bail!("--import-db 需要旧版 clipboard.db 路径");
                    };
                    options.import_db = Some(path.into());
                }
                _ => bail!("未知参数：{}", arg.to_string_lossy()),
            }
        }
        if options.smoke_test {
            if options.import_db.is_some() {
                bail!("--smoke-test 不能与 --import-db 同时使用");
            }
            if options.data_dir.is_none() {
                bail!("--smoke-test 必须指定独立的 --data-dir");
            }
            options.monitor = false;
        }
        Ok(Some(options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>> {
        Options::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn smoke_test_requires_explicit_data_and_disables_capture() -> Result<()> {
        assert!(parse(&["--smoke-test"]).is_err());
        let options = parse(&["--smoke-test", "--data-dir", "C:\\测试 目录"])?.unwrap();
        assert!(!options.monitor);
        assert_eq!(options.data_dir, Some(PathBuf::from("C:\\测试 目录")));
        assert!(parse(&["--data-dir"]).is_err());
        assert!(parse(&["--data-dir", "--no-monitor"]).is_err());
        assert!(parse(&["--unknown"]).is_err());
        assert!(parse(&["--import-db"]).is_err());
        assert!(
            parse(&[
                "--smoke-test",
                "--data-dir",
                "isolated",
                "--import-db",
                "old.db"
            ])
            .is_err()
        );
        assert_eq!(
            parse(&["--import-db", "old.db"])?.unwrap().import_db,
            Some(PathBuf::from("old.db"))
        );
        assert!(parse(&["--help"])?.is_none());
        Ok(())
    }
}
