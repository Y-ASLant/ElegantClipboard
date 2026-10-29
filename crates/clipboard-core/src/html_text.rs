//! Plain-text fallback for HTML clipboard records, using the HTML5 tokenizer.
use crate::MAX_TEXT_BYTES;
use anyhow::{Result, bail};
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
};

/// Extract visible text without wrapping or truncating the body. Source plain text
/// should take precedence over this fallback, including whitespace-only text.
pub fn html_to_text(html: &str) -> Result<String> {
    Ok(tokenize(html)?.text.into_inner())
}

/// 取条目的纯文本表示：优先正文 `text_content`，为空则把 HTML 转纯文本。
///
/// 复制/合并等需要文本回退链的场景共用。`Ok(None)` 表示没有可用文本
/// （无正文、无 HTML 或 HTML 转出空文本）；HTML 转换本身失败（如超限）
/// 仍返回 `Err`，由调用方决定提示语。
pub fn item_plain_text(item: &crate::database::ClipboardItem) -> Result<Option<Cow<'_, str>>> {
    if let Some(text) = item.text_content.as_deref().filter(|text| !text.is_empty()) {
        return Ok(Some(Cow::Borrowed(text)));
    }
    let Some(html) = item.html_content.as_deref() else {
        return Ok(None);
    };
    let text = html_to_text(html)?;
    Ok((!text.is_empty()).then_some(Cow::Owned(text)))
}

/// 带原生位图的纯图片 HTML 应优先保存像素，不以图片地址替代图片内容。
pub fn is_image_only_html(html: &str) -> Result<bool> {
    let sink = tokenize(html)?;
    Ok(sink.has_image.get() && sink.text.borrow().trim().is_empty())
}

fn tokenize(html: &str) -> Result<TextSink> {
    if html.len() > MAX_TEXT_BYTES {
        bail!("HTML 内容超过 1 MiB，无法转换为纯文本");
    }
    let input = BufferQueue::default();
    input.push_back(html.into());
    let tokenizer = Tokenizer::new(TextSink::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    if tokenizer.sink.overflow.get() {
        bail!("HTML 转换后的纯文本超过 1 MiB，未复制");
    }
    Ok(tokenizer.sink)
}

#[derive(Default)]
struct TextSink {
    text: RefCell<String>,
    pending: Cell<Option<char>>,
    hidden: RefCell<Option<String>>,
    preformatted: Cell<usize>,
    overflow: Cell<bool>,
    has_image: Cell<bool>,
}

impl TextSink {
    fn push(&self, character: char) {
        if self.overflow.get() {
            return;
        }
        let mut text = self.text.borrow_mut();
        if text.len() + character.len_utf8() > MAX_TEXT_BYTES {
            self.overflow.set(true);
        } else {
            text.push(character);
        }
    }

    fn boundary(&self, separator: char) {
        if self.text.borrow().is_empty() {
            return;
        }
        if separator == '\n' || self.pending.get() != Some('\n') {
            self.pending.set(Some(separator));
        }
    }

    fn character(&self, character: char) {
        if self.preformatted.get() == 0 && matches!(character, ' ' | '\t' | '\n' | '\r' | '\x0c') {
            if self.pending.get().is_none() {
                self.boundary(' ');
            }
            return;
        }
        if let Some(separator) = self.pending.take() {
            self.push(separator);
        }
        self.push(character);
    }
}

impl TokenSink for TextSink {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                let start = tag.kind == TagKind::StartTag;
                if self.hidden.borrow().is_some() {
                    if !start && self.hidden.borrow().as_deref() == Some(name) {
                        *self.hidden.borrow_mut() = None;
                    }
                    return TokenSinkResult::Continue;
                }
                if start && name == "img" {
                    self.has_image.set(true);
                }
                if start && matches!(name, "script" | "style" | "title") {
                    *self.hidden.borrow_mut() = Some(name.to_owned());
                    return TokenSinkResult::RawData(match name {
                        "script" => RawKind::ScriptData,
                        "title" => RawKind::Rcdata,
                        _ => RawKind::Rawtext,
                    });
                }
                if matches!(name, "pre" | "textarea") {
                    self.preformatted.set(if start {
                        self.preformatted.get() + 1
                    } else {
                        self.preformatted.get().saturating_sub(1)
                    });
                }
                if matches!(
                    name,
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "div"
                        | "dl"
                        | "dt"
                        | "dd"
                        | "fieldset"
                        | "figcaption"
                        | "figure"
                        | "footer"
                        | "form"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "header"
                        | "hr"
                        | "li"
                        | "main"
                        | "nav"
                        | "ol"
                        | "p"
                        | "pre"
                        | "section"
                        | "table"
                        | "tr"
                        | "ul"
                        | "textarea"
                ) || (start && name == "br")
                {
                    self.boundary('\n');
                } else if !start && matches!(name, "td" | "th") {
                    self.boundary('\t');
                }
                if start && name == "textarea" {
                    return TokenSinkResult::RawData(RawKind::Rcdata);
                }
            }
            Token::CharacterTokens(text) if self.hidden.borrow().is_none() => {
                for character in text.chars() {
                    self.character(character);
                }
            }
            Token::NullCharacterToken if self.hidden.borrow().is_none() => {
                self.character('\u{fffd}')
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_entities_blocks_and_table_without_script_or_style() -> Result<()> {
        let html = concat!(
            "<TITLE>Hidden title</TITLE><p title='a > b'>A &amp; B&nbsp;&#x1F600;</p>",
            "<script>if (a < b) { x = '<p>hidden</p>'; }</script>",
            "<style>.x::before { content: '<b>hidden</b>'; }</style>",
            "<!-- hidden --><p>Second<br>line</p>",
            "<table><tr><th>名</th><th>值</th></tr><tr><td>甲</td><td>&lt;2</td></tr></table>"
        );
        assert_eq!(
            html_to_text(html)?,
            "A & B\u{a0}😀\nSecond\nline\n名\t值\n甲\t<2"
        );
        Ok(())
    }

    #[test]
    fn preserves_inline_spacing_preformatted_text_and_malformed_html() -> Result<()> {
        assert_eq!(
            html_to_text("one<b>two</b>  three &NotEqualTilde;")?,
            "onetwo three ≂̸"
        );
        assert_eq!(
            html_to_text("<pre>  first\n second</pre><p>last")?,
            "  first\n second\nlast"
        );
        assert_eq!(
            html_to_text("<p>A &amp; B<script>hidden < invalid")?,
            "A & B"
        );
        Ok(())
    }

    #[test]
    fn distinguishes_image_markup_from_visible_text_and_hidden_images() -> Result<()> {
        assert!(is_image_only_html(
            "<html><body><!--StartFragment--><IMG src='blob:test' alt='图片'><!--EndFragment--></body></html>"
        )?);
        assert!(is_image_only_html("<div> \n<img src='image.png'></div>")?);
        assert!(!is_image_only_html("<p>正文<img src='image.png'></p>")?);
        assert!(!is_image_only_html(
            "<script>const x = '<img src=x>';</script>"
        )?);
        assert!(!is_image_only_html("<!-- <img src=x> --><p> </p>")?);
        Ok(())
    }

    #[test]
    fn rejects_oversized_input_and_expanded_output_without_truncation() -> Result<()> {
        assert_eq!(
            html_to_text(&"a".repeat(MAX_TEXT_BYTES))?.len(),
            MAX_TEXT_BYTES
        );
        assert!(html_to_text(&"a".repeat(MAX_TEXT_BYTES + 1)).is_err());
        // HTML replaces each NUL with U+FFFD, expanding one byte to three.
        assert!(html_to_text(&"\0".repeat(MAX_TEXT_BYTES / 3 + 1)).is_err());
        Ok(())
    }
}
