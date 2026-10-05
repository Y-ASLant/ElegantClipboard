use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};

fn main() {
	let ctx = ClipboardContext::new().unwrap();
	let types = ctx.available_formats().unwrap();
	println!("{:?}", types);

	let has_rtf = ctx.has(ContentFormat::Rtf);
	println!("has_rtf={}", has_rtf);

	let rtf = ctx.get_rich_text().unwrap_or_default();

	println!("rtf={}", rtf);

	let has_html = ctx.has(ContentFormat::Html);
	println!("has_html={}", has_html);

	let html = ctx.get_html().unwrap_or_default();

	println!("html={}", html);

	let content = ctx.get_text().unwrap_or_default();

	println!("txt={}", content);
}
