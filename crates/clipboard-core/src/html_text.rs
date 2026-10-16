//! Plain-text fallback for HTML clipboard records, using the HTML5 tokenizer.
use crate::MAX_TEXT_BYTES;
use anyhow::{Result, bail};
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
};

/// Extract visible text without wrapping or truncating the body. Source plain text
/// should take precedence over this fallback, including whitespace-only text.
pub fn html_to_text(html: &str) -> Result<String> {
    if html.len() > MAX_TEXT_BYTES {
        bail!("HTML 内容超过 1 MiB，无法转换为纯文本");
    }
    let mut input = BufferQueue::default();
    input.push_back(html.into());
    let mut tokenizer = Tokenizer::new(TextSink::default(), Default::default());
    let _ = tokenizer.feed(&mut input);
    tokenizer.end();
    if tokenizer.sink.overflow {
        bail!("HTML 转换后的纯文本超过 1 MiB，未复制");
    }
    Ok(tokenizer.sink.text)
}

#[derive(Default)]
struct TextSink {
    text: String,
    pending: Option<char>,
    hidden: Option<String>,
    preformatted: usize,
    overflow: bool,
}

impl TextSink {
    fn push(&mut self, character: char) {
        if self.overflow {
            return;
        }
        if self.text.len() + character.len_utf8() > MAX_TEXT_BYTES {
            self.overflow = true;
        } else {
            self.text.push(character);
        }
    }

    fn boundary(&mut self, separator: char) {
        if self.text.is_empty() {
            return;
        }
        if separator == '\n' || self.pending != Some('\n') {
            self.pending = Some(separator);
        }
    }

    fn character(&mut self, character: char) {
        if self.preformatted == 0 && matches!(character, ' ' | '\t' | '\n' | '\r' | '\x0c') {
            if self.pending.is_none() {
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

    fn process_token(&mut self, token: Token, _: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                let start = tag.kind == TagKind::StartTag;
                if let Some(hidden) = &self.hidden {
                    if !start && name == hidden {
                        self.hidden = None;
                    }
                    return TokenSinkResult::Continue;
                }
                if start && matches!(name, "script" | "style" | "title") {
                    self.hidden = Some(name.to_owned());
                    return TokenSinkResult::RawData(match name {
                        "script" => RawKind::ScriptData,
                        "title" => RawKind::Rcdata,
                        _ => RawKind::Rawtext,
                    });
                }
                if matches!(name, "pre" | "textarea") {
                    self.preformatted = if start {
                        self.preformatted + 1
                    } else {
                        self.preformatted.saturating_sub(1)
                    };
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
            Token::CharacterTokens(text) if self.hidden.is_none() => {
                for character in text.chars() {
                    self.character(character);
                }
            }
            Token::NullCharacterToken if self.hidden.is_none() => self.character('\u{fffd}'),
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
