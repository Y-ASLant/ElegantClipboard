use std::path::{Path, PathBuf};

pub(super) fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn arguments(exe: &str) -> String {
    format!("<array><string>{}</string></array>", xml(exe))
}

pub(super) fn environment(data_dir: &str) -> String {
    format!(
        "<dict>        <key>{hidden}</key><string>1</string>        <key>{data}</key><string>{dir}</string>        </dict>",
        hidden = crate::launch_env::START_HIDDEN,
        data = crate::launch_env::DATA_DIR,
        dir = xml(data_dir),
    )
}

struct Node<'a> {
    tag: &'a str,
    text: String,
    children: Vec<Node<'a>>,
}

fn unescape(value: &str) -> Option<String> {
    let mut result = String::new();
    let mut remaining = value;
    while let Some(index) = remaining.find('&') {
        result.push_str(&remaining[..index]);
        remaining = &remaining[index + 1..];
        let (entity, tail) = remaining.split_once(';')?;
        result.push(match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let number = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    entity.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(number)?
            }
        });
        remaining = tail;
    }
    result.push_str(remaining);
    (!result.contains('\0')).then_some(result)
}

struct Reader<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Reader<'a> {
    fn skip(&mut self) -> Option<()> {
        loop {
            self.offset +=
                self.source[self.offset..].len() - self.source[self.offset..].trim_start().len();
            let tail = &self.source[self.offset..];
            let end = if tail.starts_with("<?xml ") {
                tail.find("?>")? + 2
            } else if tail.starts_with("<!--") {
                tail.find("-->")? + 3
            } else if tail.starts_with("<!DOCTYPE plist ") {
                let end = tail.find('>')? + 1;
                if tail[..end].contains('[') {
                    return None;
                }
                end
            } else {
                break;
            };
            self.offset += end;
        }
        Some(())
    }

    fn node(&mut self, depth: usize) -> Option<Node<'a>> {
        if depth > 32 {
            return None;
        }
        self.skip()?;
        let tail = self.source[self.offset..].strip_prefix('<')?;
        let length = tail.find('>')?;
        let opening = &tail[..length];
        let empty = opening.ends_with('/');
        let opening = opening.trim_end_matches('/').trim();
        let tag = if opening == "plist version=\"1.0\"" {
            "plist"
        } else {
            opening
        };
        if !matches!(
            tag,
            "plist" | "dict" | "array" | "key" | "string" | "true" | "false" | "integer"
        ) {
            return None;
        }
        self.offset += length + 2;
        let mut children = Vec::new();
        let mut text = String::new();
        if !empty {
            loop {
                if self.source[self.offset..].starts_with("</") {
                    let closing = format!("</{tag}>");
                    if !self.source[self.offset..].starts_with(&closing) {
                        return None;
                    }
                    self.offset += closing.len();
                    break;
                }
                if self.source[self.offset..].starts_with("<!--") {
                    self.skip()?;
                    continue;
                }
                if self.source[self.offset..].starts_with('<') {
                    children.push(self.node(depth + 1)?);
                } else {
                    let end = self.source[self.offset..].find('<')? + self.offset;
                    text.push_str(&unescape(&self.source[self.offset..end])?);
                    self.offset = end;
                }
            }
        }
        Some(Node {
            tag,
            text,
            children,
        })
    }
}

pub(super) struct Entry {
    pub executable: PathBuf,
    pub enabled: bool,
}

pub(super) fn parse(content: &str, label: &str, data_dir: &Path) -> Option<Entry> {
    let mut reader = Reader {
        source: content,
        offset: 0,
    };
    let root = reader.node(0)?;
    reader.skip()?;
    if reader.offset != content.len()
        || root.tag != "plist"
        || !root.text.trim().is_empty()
        || root.children.len() != 1
    {
        return None;
    }
    let dict = &root.children[0];
    if dict.tag != "dict" || !dict.text.trim().is_empty() || !dict.children.len().is_multiple_of(2)
    {
        return None;
    }
    let mut keys = std::collections::HashSet::new();
    let mut label_seen = false;
    let mut arguments_seen = false;
    let mut environment_seen = false;
    let mut executable = None;
    let mut disabled = false;
    for pair in dict.children.as_chunks::<2>().0 {
        let [key, value] = pair;
        if key.tag != "key" || !key.children.is_empty() || !keys.insert(key.text.as_str()) {
            return None;
        }
        match key.text.as_str() {
            "Label" => {
                if value.tag != "string" || !value.children.is_empty() || value.text != label {
                    return None;
                }
                label_seen = true;
            }
            "ProgramArguments" => {
                if value.tag != "array"
                    || !value.text.trim().is_empty()
                    || value.children.len() != 1
                {
                    return None;
                }
                let arg = &value.children[0];
                if arg.tag != "string" || !arg.children.is_empty() {
                    return None;
                }
                let exe = Path::new(&arg.text);
                if !exe.is_absolute() || !matches!(exe.file_name()?.to_str()?, "ElegantClipboard") {
                    return None;
                }
                executable = Some(exe.to_path_buf());
                arguments_seen = true;
            }
            "EnvironmentVariables" => {
                if value.tag != "dict" || !value.text.trim().is_empty() {
                    return None;
                }
                let mut hidden = false;
                let mut data_ok = false;
                for pair in value.children.as_chunks::<2>().0 {
                    let (key, item) = (&pair[0], &pair[1]);
                    if key.tag != "key" || item.tag != "string" {
                        return None;
                    }
                    match key.text.as_str() {
                        name if name == crate::launch_env::START_HIDDEN => {
                            if item.text != "1" {
                                return None;
                            }
                            hidden = true;
                        }
                        name if name == crate::launch_env::DATA_DIR => {
                            if Path::new(&item.text) != data_dir {
                                return None;
                            }
                            data_ok = true;
                        }
                        _ => return None,
                    }
                }
                if !hidden || !data_ok {
                    return None;
                }
                environment_seen = true;
            }
            "Program" => return None,
            "Disabled" => {
                if !matches!(value.tag, "true" | "false")
                    || !value.text.is_empty()
                    || !value.children.is_empty()
                {
                    return None;
                }
                disabled = value.tag == "true";
            }
            _ => (),
        }
    }
    if !label_seen || !arguments_seen || !environment_seen {
        return None;
    }
    Some(Entry {
        executable: executable?,
        enabled: !disabled,
    })
}
