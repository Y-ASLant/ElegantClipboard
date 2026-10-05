use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, ContentFormat};

#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn test_file() {
	let directory = tempfile::tempdir().unwrap();
	let file_list: Vec<String> = ["clipboard_rs_test_file1.txt", "clipboard_rs_test_file2.txt"]
		.iter()
		.map(|name| {
			let path = directory.path().join(name);
			std::fs::write(&path, "hello world").unwrap();
			path.to_str().unwrap().to_string()
		})
		.collect();
	let ctx = ClipboardContext::new().unwrap();

	ctx.set_files(file_list.clone()).unwrap();
	assert!(ctx.has(ContentFormat::Files));
	assert_eq!(ctx.get_files().unwrap(), file_list);
	#[cfg(target_os = "windows")]
	{
		let raw_hdrop = ctx.get_hdrop_raw().unwrap();
		ctx.clear().unwrap();
		ctx.set_hdrop_raw(&raw_hdrop).unwrap();
		ctx.set_raw_no_clear("Preferred DropEffect", &1_u32.to_le_bytes())
			.unwrap();
		assert_eq!(ctx.get_hdrop_raw().unwrap(), raw_hdrop);
		assert_eq!(ctx.get_files().unwrap(), file_list);
		assert_eq!(
			ctx.get_buffer("Preferred DropEffect").unwrap(),
			1_u32.to_le_bytes()
		);
	}

	ctx.clear().unwrap();
	assert!(!ctx.has(ContentFormat::Files));

	let text = file_list.join("\n");
	ctx.set(vec![
		ClipboardContent::Text(text.clone()),
		ClipboardContent::Files(file_list.clone()),
	])
	.unwrap();
	assert!(ctx.has(ContentFormat::Files));

	let contents = ctx
		.get(&[ContentFormat::Text, ContentFormat::Files])
		.unwrap();
	assert_eq!(contents.len(), 2);
	for content in contents {
		match content {
			ClipboardContent::Text(data) => assert_eq!(data, text),
			ClipboardContent::Files(files) => assert_eq!(files, file_list),
			_ => panic!("unexpected format"),
		}
	}
	ctx.clear().unwrap();
}
