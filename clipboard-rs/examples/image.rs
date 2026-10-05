#[cfg(target_os = "linux")]
use clipboard_rs::ClipboardContextX11Options;
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};

#[cfg(target_os = "linux")]
fn setup_clipboard() -> ClipboardContext {
	ClipboardContext::new_with_options(ClipboardContextX11Options { read_timeout: None }).unwrap()
}

#[cfg(not(target_os = "linux"))]
fn setup_clipboard() -> ClipboardContext {
	ClipboardContext::new().unwrap()
}

fn main() {
	let ctx = setup_clipboard();
	let temporary_directory = std::env::temp_dir();

	let types = ctx.available_formats().unwrap();
	println!("{:?}", types);

	let img = ctx.get_image();

	match img {
		Ok(img) => {
			let path = temporary_directory.join("clipboard-rs-test.png");
			let _ = img
				.save_to_path(path.to_str().unwrap())
				.map_err(|e| println!("save test.png err={}", e));

			let resize_img = img
				.thumbnail(300, 300)
				.map_err(|e| println!("thumbnail err={}", e))
				.unwrap();

			let path = temporary_directory.join("clipboard-rs-test-thumbnail.png");
			let _ = resize_img
				.save_to_path(path.to_str().unwrap())
				.map_err(|e| println!("save test_thumbnail.png err={}", e));
		}
		Err(err) => {
			println!("err={}", err);
		}
	}
}
