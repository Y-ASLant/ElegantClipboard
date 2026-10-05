use clipboard_rs::{Clipboard, ClipboardContext};

fn main() {
	let ctx = ClipboardContext::new().unwrap();
	let types = ctx.available_formats().unwrap();
	println!("{:?}", types);

	let format = if cfg!(target_os = "windows") {
		"HTML Format"
	} else if cfg!(any(target_os = "macos", target_os = "ios")) {
		"public.html"
	} else {
		"text/html"
	};
	let buffer = ctx.get_buffer(format).unwrap();

	let string = String::from_utf8(buffer).unwrap();

	println!("{}", string);
}
