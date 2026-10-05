use clipboard_rs::{
	common::ContentData, Clipboard, ClipboardContent, ClipboardContext, ContentFormat,
};

#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn test_string() {
	let ctx = ClipboardContext::new().unwrap();
	ctx.clear().unwrap();

	let test_plain_txt = "hell@$#%^&U都98好的😊o Rust!!!";
	ctx.set_text(test_plain_txt.to_string()).unwrap();
	assert!(ctx.has(ContentFormat::Text));
	assert_eq!(ctx.get_text().unwrap(), test_plain_txt);

	let test_rich_txt = "{\\rtf1\\ansi Hello, Rust!}";
	ctx.set_rich_text(test_rich_txt.to_string()).unwrap();
	assert!(ctx.has(ContentFormat::Rtf));
	assert_eq!(ctx.get_rich_text().unwrap(), test_rich_txt);

	let test_html = "<html><body><h1>Hello, Rust!</h1></body></html>";
	ctx.set_html(test_html.to_string()).unwrap();
	assert!(ctx.has(ContentFormat::Html));
	assert_eq!(ctx.get_html().unwrap(), test_html);

	let contents: Vec<ClipboardContent> = vec![
		ClipboardContent::Text(test_plain_txt.to_string()),
		ClipboardContent::Rtf(test_rich_txt.to_string()),
		ClipboardContent::Html(test_html.to_string()),
	];
	ctx.set(contents).unwrap();
	assert!(ctx.has(ContentFormat::Text));
	assert!(ctx.has(ContentFormat::Rtf));
	assert!(ctx.has(ContentFormat::Html));
	assert_eq!(ctx.get_text().unwrap(), test_plain_txt);
	assert_eq!(ctx.get_rich_text().unwrap(), test_rich_txt);
	assert_eq!(ctx.get_html().unwrap(), test_html);

	let content_arr = ctx
		.get(&[ContentFormat::Text, ContentFormat::Rtf, ContentFormat::Html])
		.unwrap();

	assert_eq!(content_arr.len(), 3);
	for c in content_arr {
		let content_str = c.as_str().unwrap();
		match c.get_format() {
			ContentFormat::Text => assert_eq!(content_str, test_plain_txt),
			ContentFormat::Rtf => assert_eq!(content_str, test_rich_txt),
			ContentFormat::Html => assert_eq!(content_str, test_html),
			_ => panic!("unexpected format"),
		}
	}
}

#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
#[cfg(target_os = "macos")]
fn test_set_multiple_formats_is_one_item_macos() {
	// Import macOS-specific types needed for verification
	use objc2::rc::autoreleasepool;
	use objc2_app_kit::{
		NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
	};

	let ctx = ClipboardContext::new().unwrap();

	ctx.clear().unwrap();

	let test_plain_txt = "Hello Text";
	let test_rich_txt = "{\\rtf1 Hello RTF}";
	let test_html = "<h1>Hello HTML</h1>";

	let contents: Vec<ClipboardContent> = vec![
		ClipboardContent::Text(test_plain_txt.to_string()),
		ClipboardContent::Rtf(test_rich_txt.to_string()),
		ClipboardContent::Html(test_html.to_string()),
	];

	// Action: Set the clipboard with multiple content types
	ctx.set(contents).unwrap();

	// Verification: Directly inspect the NSPasteboard to check the number of items.
	// The correct behavior is to have ONE item with multiple representations.
	// The buggy behavior creates THREE separate items.
	autoreleasepool(|_| {
		let pasteboard = NSPasteboard::generalPasteboard();
		let items = pasteboard
			.pasteboardItems()
			.expect("Failed to get pasteboard items for verification");

		// [THIS IS THE KEY ASSERTION]
		// It will fail on the original code because `items.count()` will be 3.
		// It will pass on the fixed code because `items.count()` will be 1.
		assert_eq!(
			items.count(),
			1,
			"Setting multiple formats should create a single pasteboard item, but it created {}",
			items.count()
		);

		// [BONUS ASSERTIONS]
		// We can also verify that the single item contains all the correct types.
		let item = items.objectAtIndex(0);
		let types = item.types();

		assert!(
			unsafe { types.containsObject(NSPasteboardTypeString) },
			"The single pasteboard item should contain the String type"
		);
		assert!(
			unsafe { types.containsObject(NSPasteboardTypeRTF) },
			"The single pasteboard item should contain the RTF type"
		);
		assert!(
			unsafe { types.containsObject(NSPasteboardTypeHTML) },
			"The single pasteboard item should contain the HTML type"
		);
	});
}

#[test]
fn content_bytes_and_strings_preserve_payloads() {
	let text = "中文 café 😀";
	for content in [
		ClipboardContent::Text(text.to_string()),
		ClipboardContent::Rtf(text.to_string()),
		ClipboardContent::Html(text.to_string()),
		ClipboardContent::Other("custom".to_string(), text.as_bytes().to_vec()),
	] {
		assert_eq!(content.as_bytes(), text.as_bytes());
		assert_eq!(content.as_str().unwrap(), text);
	}
}

#[test]
fn binary_and_empty_file_content_are_not_strings() {
	let binary = ClipboardContent::Other("custom".to_string(), vec![0xff, 0x00]);
	assert_eq!(binary.as_bytes(), &[0xff, 0x00]);
	assert!(binary.as_str().is_err());
	let files = ClipboardContent::Files(Vec::new());
	assert!(files.as_bytes().is_empty());
	assert!(files.as_str().is_err());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn operations_fail_when_another_thread_holds_the_clipboard() {
	use std::ffi::c_void;

	#[link(name = "user32")]
	extern "system" {
		#[link_name = "CreateWindowExW"]
		fn create_window_ex_w(
			extended_style: u32,
			class_name: *const u16,
			window_name: *const u16,
			style: u32,
			x: i32,
			y: i32,
			width: i32,
			height: i32,
			parent: *mut c_void,
			menu: *mut c_void,
			instance: *mut c_void,
			parameter: *mut c_void,
		) -> *mut c_void;
		#[link_name = "DestroyWindow"]
		fn destroy_window(window: *mut c_void) -> i32;
	}

	struct OwnerWindow(*mut c_void);
	impl Drop for OwnerWindow {
		fn drop(&mut self) {
			// The fixture stays on the creating thread; no clipboard guard survives it.
			unsafe {
				destroy_window(self.0);
			}
		}
	}

	let class_name = [
		b'S' as u16,
		b'T' as u16,
		b'A' as u16,
		b'T' as u16,
		b'I' as u16,
		b'C' as u16,
		0,
	];
	// STATIC is predefined; HWND_MESSAGE (-3) creates a message-only window.
	// The class name is terminated, and all optional native arguments are null.
	let window = unsafe {
		create_window_ex_w(
			0,
			class_name.as_ptr(),
			std::ptr::null(),
			0,
			0,
			0,
			0,
			0,
			-3_isize as *mut c_void,
			std::ptr::null_mut(),
			std::ptr::null_mut(),
			std::ptr::null_mut(),
		)
	};
	assert!(
		!window.is_null(),
		"message-only owner creation failed: {}",
		std::io::Error::last_os_error()
	);
	let owner = OwnerWindow(window);
	let ctx = ClipboardContext::new().unwrap();
	let original = "clipboard must survive a rejected write";
	let directory = tempfile::tempdir().unwrap();
	let path = directory.path().join("clipboard-rs-contention.txt");
	std::fs::write(&path, original).unwrap();
	ctx.set_files(vec![path.to_str().unwrap().to_string()])
		.unwrap();
	let raw_hdrop = ctx.get_hdrop_raw().unwrap();
	ctx.set_text(original.to_string()).unwrap();
	let guard = clipboard_win::Clipboard::new_attempts_for(owner.0, 10).unwrap();
	let worker = std::thread::spawn(move || {
		let ctx = ClipboardContext::new().unwrap();
		[
			ctx.clear().unwrap_err(),
			ctx.available_formats().unwrap_err(),
			ctx.set_text("must not write".to_string()).unwrap_err(),
			ctx.set_buffer("clipboard-rs-test", vec![1]).unwrap_err(),
			ctx.set_rich_text("{\\rtf1 must not write}".to_string())
				.unwrap_err(),
			ctx.set_html("<p>must not write</p>".to_string())
				.unwrap_err(),
			ctx.set_files(vec!["C:\\clipboard-rs-test.txt".to_string()])
				.unwrap_err(),
			ctx.set(vec![ClipboardContent::Text("must not write".to_string())])
				.unwrap_err(),
			ctx.set_hdrop_raw(&raw_hdrop).unwrap_err(),
			ctx.set_raw_no_clear("clipboard-rs-test", &[1]).unwrap_err(),
			ctx.get_hdrop_raw().unwrap_err(),
		]
	});
	let rejected = worker.join();
	drop(guard);
	drop(owner);
	assert!(rejected
		.unwrap()
		.iter()
		.all(|error| error.is::<clipboard_rs::ClipboardAccessError>()));
	assert_eq!(ctx.get_text().unwrap(), original);
	ctx.clear().unwrap();
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn zero_byte_raw_publication_is_unsupported_without_mutating_clipboard() {
	let ctx = ClipboardContext::new().unwrap();
	let format = "clipboard-rs-zero-byte-buffer";
	let sentinel = "companion content must survive";
	ctx.set_text(sentinel.to_string()).unwrap();
	let error = ctx.set_buffer(format, Vec::new()).unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	assert!(!ctx.has(ContentFormat::Other(format.to_string())));
	let error = ctx.set_raw_no_clear(format, &[]).unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	let error = ctx
		.set(vec![
			ClipboardContent::Text("must not replace the sentinel".to_string()),
			ClipboardContent::Other(format.to_string(), Vec::new()),
		])
		.unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	let error = ctx.set_hdrop_raw(&[]).unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	let error = ctx.set_rich_text(String::new()).unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	#[cfg(feature = "image")]
	{
		use clipboard_rs::{common::RustImage, RustImageData};
		let image = RustImageData::from_bytes(include_bytes!("test.png")).unwrap();
		let error = ctx.set_image_with_dib(image, Some(&[])).unwrap_err();
		assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
		assert_eq!(ctx.get_text().unwrap(), sentinel);
	}
	assert!(!ctx.has(ContentFormat::Other(format.to_string())));
	let payload = [0, 0xff, 1];
	ctx.set_raw_no_clear(format, &payload).unwrap();
	assert!(ctx.has(ContentFormat::Other(format.to_string())));
	assert_eq!(ctx.get_buffer(format).unwrap(), payload);
	assert_eq!(ctx.get_text().unwrap(), sentinel);
	let error = ctx.set_raw_no_clear(format, &[]).unwrap_err();
	assert!(error.is::<clipboard_rs::ClipboardFormatUnsupportedError>());
	assert_eq!(ctx.get_buffer(format).unwrap(), payload);
	ctx.set_buffer(format, payload.to_vec()).unwrap();
	assert!(!ctx.has(ContentFormat::Text));
	assert_eq!(ctx.get_buffer(format).unwrap(), payload);
	ctx.clear().unwrap();
	assert!(!ctx.has(ContentFormat::Other(format.to_string())));
	assert!(ctx.get_buffer(format).is_err());
	ctx.set_text(String::new()).unwrap();
	assert!(ctx.has(ContentFormat::Text));
	assert!(ctx.get_text().unwrap().is_empty());
	ctx.clear().unwrap();
}
