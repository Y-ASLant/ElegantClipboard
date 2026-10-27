//! 只定位实际消费的路径字符串，其余 JSON 字节保持原样。
use super::paths::Relocation;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Clone, Copy)]
enum Field {
    Ignore,
    Files,
    Payload,
    Staged,
    Entry,
    Path,
}

pub(super) fn remap(raw: &str, payload: bool, relocation: &Relocation) -> Result<Option<String>> {
    // 由 serde 负责完整语法校验；IgnoredAny 无需转换数字，
    // 因而不会舍入或限制未知字段中的数值。下方单独限制遍历深度。
    let mut validator = serde_json::Deserializer::from_str(raw);
    serde::de::IgnoredAny::deserialize(&mut validator).context("路径 JSON 无效")?;
    validator.end().context("路径 JSON 含多余内容")?;
    let mut walker = Walker {
        raw,
        position: 0,
        relocation,
        edits: Vec::new(),
        depth: 0,
    };
    walker.value(if payload {
        Field::Payload
    } else {
        Field::Files
    })?;
    if walker.edits.is_empty() {
        return Ok(None);
    }
    let mut output = String::with_capacity(raw.len());
    let mut previous = 0;
    for (start, end, replacement) in walker.edits {
        output.push_str(&raw[previous..start]);
        output.push_str(&replacement);
        previous = end;
    }
    output.push_str(&raw[previous..]);
    Ok(Some(output))
}

struct Walker<'a> {
    raw: &'a str,
    position: usize,
    relocation: &'a Relocation,
    edits: Vec<(usize, usize, String)>,
    depth: usize,
}

impl Walker<'_> {
    fn whitespace(&mut self) {
        while self
            .raw
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn string(&mut self) -> (usize, usize) {
        let start = self.position;
        self.position += 1;
        loop {
            match self.raw.as_bytes()[self.position] {
                b'\\' => self.position += 2,
                b'"' => {
                    self.position += 1;
                    return (start, self.position);
                }
                _ => self.position += 1,
            }
        }
    }

    fn value(&mut self, field: Field) -> Result<()> {
        ensure!(self.depth < 128, "路径 JSON 嵌套过深");
        self.depth += 1;
        self.whitespace();
        let token = self.raw.as_bytes()[self.position];
        let valid_shape = match field {
            Field::Files | Field::Staged => token == b'[',
            Field::Payload | Field::Entry => token == b'{',
            Field::Path => token == b'"' || token == b'n',
            Field::Ignore => true,
        };
        ensure!(valid_shape, "路径 JSON 字段类型无效");
        match token {
            b'{' => self.object(field)?,
            b'[' => self.array(field)?,
            b'"' => {
                let (start, end) = self.string();
                if matches!(field, Field::Path) {
                    let path: String = serde_json::from_str(&self.raw[start..end])?;
                    if let Some(mapped) = self.relocation.map(&path) {
                        self.edits
                            .push((start, end, serde_json::to_string(&mapped)?));
                    }
                }
            }
            _ => {
                while self.raw.as_bytes().get(self.position).is_some_and(|byte| {
                    !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}')
                }) {
                    self.position += 1;
                }
            }
        }
        self.depth -= 1;
        Ok(())
    }

    fn object(&mut self, field: Field) -> Result<()> {
        self.position += 1;
        self.whitespace();
        while self.raw.as_bytes()[self.position] != b'}' {
            let (start, end) = self.string();
            let child = if matches!(field, Field::Payload | Field::Entry) {
                let key: String = serde_json::from_str(&self.raw[start..end])?;
                match (field, key.as_str()) {
                    (Field::Payload, "staged") => Field::Staged,
                    (Field::Entry, "staged" | "original") => Field::Path,
                    _ => Field::Ignore,
                }
            } else {
                Field::Ignore
            };
            self.whitespace();
            self.position += 1; // 已校验的 ':'
            self.value(child)?;
            self.whitespace();
            if self.raw.as_bytes()[self.position] == b',' {
                self.position += 1;
                self.whitespace();
            }
        }
        self.position += 1;
        Ok(())
    }

    fn array(&mut self, field: Field) -> Result<()> {
        self.position += 1;
        self.whitespace();
        while self.raw.as_bytes()[self.position] != b']' {
            self.value(match field {
                Field::Files => Field::Path,
                Field::Staged => Field::Entry,
                _ => Field::Ignore,
            })?;
            self.whitespace();
            if self.raw.as_bytes()[self.position] == b',' {
                self.position += 1;
            }
            self.whitespace();
        }
        self.position += 1;
        Ok(())
    }
}
