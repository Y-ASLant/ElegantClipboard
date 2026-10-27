use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub(super) fn quote(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .context("Autostart paths must be valid UTF-8")?;
    if value.contains(['\n', '\r', '\0']) {
        bail!("Autostart paths cannot contain line breaks or NUL");
    }
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(quoted)
}

pub(super) fn exec(exe: &Path, data_dir: &Path) -> Result<String> {
    Ok(format!(
        "Exec={} --start-hidden --data-dir {}",
        quote(exe)?,
        quote(data_dir)?
    ))
}

fn arguments(value: &str) -> Option<Vec<String>> {
    let mut decoded = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        decoded.push(if character == '\\' {
            match chars.next()? {
                '\\' => '\\',
                's' => ' ',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => return None,
            }
        } else {
            character
        });
    }
    let mut chars = decoded.chars().peekable();
    let mut result = Vec::new();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut argument = String::new();
        let mut quoted = false;
        while let Some(character) = chars.next() {
            match character {
                '"' => quoted = !quoted,
                '\\' => {
                    let escaped = chars.next()?;
                    if !quoted || !matches!(escaped, '\\' | '"' | '$' | '`') {
                        return None;
                    }
                    argument.push(escaped);
                }
                '%' => {
                    if chars.next()? != '%' {
                        return None;
                    }
                    argument.push('%');
                }
                c if c.is_ascii_whitespace() && !quoted => break,
                c => argument.push(c),
            }
        }
        if quoted {
            return None;
        }
        result.push(argument);
    }
    Some(result)
}

pub(super) struct Entry {
    pub exec_line: usize,
    pub executable: PathBuf,
    pub enabled: bool,
}

pub(super) fn parse(content: &str, data_dir: &Path) -> Option<Entry> {
    let mut active = false;
    let mut group_seen = false;
    let mut application = None;
    let mut name = None;
    let mut exec_line = None;
    let mut executable = None;
    let mut hidden = None;
    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
            if active && std::mem::replace(&mut group_seen, true) {
                return None;
            }
            continue;
        }
        if !active || line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        match key.trim() {
            "Type" => {
                if application.replace(value == "Application").is_some() {
                    return None;
                }
            }
            "Name" => {
                if name.replace(value == "ElegantClipboard").is_some() {
                    return None;
                }
            }
            "Hidden" => {
                if !matches!(value, "true" | "false") || hidden.replace(value == "true").is_some() {
                    return None;
                }
            }
            "Exec" => {
                if exec_line.replace(index).is_some() {
                    return None;
                }
                let args = arguments(value)?;
                if args.len() != 4
                    || args[1] != "--start-hidden"
                    || args[2] != "--data-dir"
                    || Path::new(&args[3]) != data_dir
                {
                    return None;
                }
                let exe = Path::new(&args[0]);
                if !exe.is_absolute()
                    || !matches!(
                        exe.file_name()?.to_str()?,
                        "elegant-clipboard-gpui" | "ElegantClipboard"
                    )
                {
                    return None;
                }
                executable = Some(exe.to_path_buf());
            }
            _ => (),
        }
    }
    if application != Some(true) || name != Some(true) {
        return None;
    }
    Some(Entry {
        exec_line: exec_line?,
        executable: executable?,
        enabled: !hidden.unwrap_or(false),
    })
}

pub(super) fn relocate(content: &str, entry: &Entry, exe: &Path, new_dir: &Path) -> Result<String> {
    let replacement = exec(exe, new_dir)?;
    Ok(content
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == entry.exec_line {
                replacement.as_str()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}
