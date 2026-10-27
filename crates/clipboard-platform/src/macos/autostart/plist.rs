use std::{
    ops::Range,
    path::{Path, PathBuf},
};

pub(super) fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn arguments(exe: &str, data_dir: &str) -> String {
    format!(
        "<array><string>{}</string><string>--start-hidden</string>\
        <string>--data-dir</string><string>{}</string></array>",
        xml(exe),
        xml(data_dir)
    )
}

struct Node<'a> {
    tag: &'a str,
    text: String,
    children: Vec<Node<'a>>,
    range: Range<usize>,
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
        let start = self.offset;
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
            range: start..self.offset,
        })
    }
}

pub(super) struct Entry {
    label: Range<usize>,
    arguments: Range<usize>,
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
    let mut saved_label = None;
    let mut saved_arguments = None;
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
                saved_label = Some(value.range.clone());
            }
            "ProgramArguments" => {
                if value.tag != "array"
                    || !value.text.trim().is_empty()
                    || value.children.len() != 4
                {
                    return None;
                }
                let args = &value.children;
                if args
                    .iter()
                    .any(|arg| arg.tag != "string" || !arg.children.is_empty())
                    || args[1].text != "--start-hidden"
                    || args[2].text != "--data-dir"
                    || Path::new(&args[3].text) != data_dir
                {
                    return None;
                }
                let exe = Path::new(&args[0].text);
                if !exe.is_absolute()
                    || !matches!(
                        exe.file_name()?.to_str()?,
                        "elegant-clipboard-gpui" | "ElegantClipboard"
                    )
                {
                    return None;
                }
                executable = Some(exe.to_path_buf());
                saved_arguments = Some(value.range.clone());
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
    Some(Entry {
        label: saved_label?,
        arguments: saved_arguments?,
        executable: executable?,
        enabled: !disabled,
    })
}

pub(super) fn relocate(
    content: &str,
    entry: &Entry,
    label: &str,
    exe: &str,
    data_dir: &str,
) -> String {
    let mut replacements = [
        (
            entry.label.clone(),
            format!("<string>{}</string>", xml(label)),
        ),
        (entry.arguments.clone(), arguments(exe, data_dir)),
    ];
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = content.to_owned();
    for (range, replacement) in replacements {
        result.replace_range(range, &replacement);
    }
    result
}
