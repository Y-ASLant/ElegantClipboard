# Upstream Changelog

The versioned entries below preserve the upstream release history, issue links and contributor attribution supplied with this vendored crate. Only the explicitly labeled local-fork maintenance section describes ElegantClipboard changes. The checked-in package declares version `0.3.5`; no missing upstream release notes are inferred or reconstructed here. Local APIs and platform limitations are documented in [README.md](README.md) / [README_ZH.md](README_ZH.md).

下列带版本号的条目保留随库提供的上游历史发布记录、issue 链接与贡献者归属；只有明确标记的本地分叉维护章节描述 ElegantClipboard 修改。当前包声明版本 `0.3.5`；这里不推测或补写缺失的上游发布内容。本地 API 与平台限制见中英文 README。

## Local Fork Maintenance / 本地分叉维护（非上游发布）

These notes describe the checked-in fork, not a new upstream `v0.3.5` release or cross-platform verification report.

### en

- Windows image publication uses PNG plus raw `CF_DIB`, preserving the complete DIB header and masks; Windows synthesizes `CF_BITMAP`. Removed the separate unsafe GDI bitmap writer and its Windows API dependency.
- Decode headerless DIB through a borrowed reader with a 14-byte BMP prefix and explicit pixel offset, avoiding a whole-DIB copy solely to add the prefix and accounting for palettes, external masks and V4/V5 headers.
- Export Windows `ClipboardAccessError` for actual guarded `OpenClipboard` failures, preserving the native source without classifying errors by display text.
- Restore Linux dispatcher forwarding for `get_image_dib()` / `set_image_with_dib()`. Non-Windows image writers reject supplied raw DIB before modifying the clipboard instead of silently dropping it; ordinary `None` image writes remain supported.
- Reject zero-byte Windows raw payloads before clipboard mutation with exported `ClipboardFormatUnsupportedError`; zero-size movable `HGLOBAL` publication via `SetClipboardData` failed on Windows. ElegantClipboard preflight maps empty raw formats, including virtual `FileContents`, to `unsupported_content`. Nonempty raw formats and the clearing/no-clear distinction remain supported; no padding, successful no-op, delayed-rendering substitution or OLE expansion is introduced. Final native nonempty/empty-rejection verification is not claimed here.
- Keep native OS-global clipboard/watcher tests opt-in and serial in an isolated desktop session. Pure content/image/header/error tests remain automatic; document current-host `--all-targets`, default/no-default-feature and strict Clippy checks in both READMEs. No Linux/macOS/iOS runtime proof is asserted.

### zh

- Windows 图片发布采用 PNG 与原始 `CF_DIB`，保留完整 DIB 头及掩码，由 Windows 合成 `CF_BITMAP`。移除单独的非安全 GDI 位图写入器及其 Windows API 依赖。
- 用借用原始 DIB 的 reader 提供 14 字节 BMP 文件头与明确像素偏移，避免仅为加文件头复制完整 DIB，并正确计算调色板、外置掩码及 V4/V5 头。
- 为真实的守卫式 `OpenClipboard` 失败导出 Windows `ClipboardAccessError`，保留原生原因，不通过显示文本匹配错误类型。
- 恢复 Linux 分发层对 `get_image_dib()` / `set_image_with_dib()` 的转发。非 Windows 图片写入器在修改剪贴板前拒绝传入的原始 DIB，不再静默丢弃；普通 `None` 图片写入仍支持。
- 在修改剪贴板前以导出的 `ClipboardFormatUnsupportedError` 拒绝 Windows 零字节原始 payload；实际通过 `SetClipboardData` 发布零字节可移动 `HGLOBAL` 在 Windows 上失败。ElegantClipboard 预检将空原始格式（包括虚拟文件 `FileContents`）映射为 `unsupported_content`。非空原始格式及清空/不清空区别仍支持；不填充、不成功但无操作、不替换为延迟渲染，也不另加 OLE 实现。这里不宣称已完成非空写入/空数据拒绝的最终原生验证。
- 系统全局剪贴板/watcher 原生测试改为显式启用，并在隔离桌面会话串行运行；纯内容、图片、头及错误测试仍自动运行。中英文 README 说明当前主机的 `--all-targets`、默认/禁用默认 feature 与严格 Clippy 检查，不宣称 Linux/macOS/iOS 运行验证。

## v0.3.4 (2026-04-02) [released]

- Fix: Convert HTML to Windows CF_HTML format when setting multiple clipboard contents, fixing malformed HTML data in `set(Vec<ClipboardContent>)` on Windows [#80](https://github.com/ChurchTao/clipboard-rs/issues/80)
- Merge pull request [#83](https://github.com/ChurchTao/clipboard-rs/pull/83) — feat: add Wayland clipboard support via wl-clipboard-rs

## v0.3.2 (2026-01-20) [released]

- Fix: Fixed HTML parsing issue

## v0.3.1 (2025-11-11) [released]

- Merge pull request [#69](https://github.com/ChurchTao/clipboard-rs/pull/69) from Ciubix8513/master
- Separate image into a dedicated crate feature
- Optimized the implementation of file-related APIs on macOS

## v0.3.0 (2025-07-01) [released]

- Add: Support iOS, but only for test, not for production.
- Fix: Merge [pr#62](https://github.com/ChurchTao/clipboard-rs/pull/62)

## v0.2.4 (2025-02-07) [released]

- Fix: [pr#56](https://github.com/ChurchTao/clipboard-rs/pull/56)

## v0.2.3 (2025-02-07) [released]

- Fix: [issues#57](https://github.com/ChurchTao/clipboard-rs/issues/57)
- Fix: [pr#56](https://github.com/ChurchTao/clipboard-rs/pull/56)

## v0.2.2 (2024-11-19) [released]

- Convergence dep: `image` to `jpeg/png/tiff/bmp` [pr#54](https://github.com/ChurchTao/clipboard-rs/pull/54)

## v0.2.1 (2024-08-26) [released]

### zh

- 增加 X11 启动参数，自定义读取的超时时间 [issues#45](https://github.com/ChurchTao/clipboard-rs/issues/45)

### en

- Add X11 startup parameters to customize the timeout for reading [issues#45](https://github.com/ChurchTao/clipboard-rs/issues/45)

## v0.2.0 (2024-08-25) [released]

### zh

- macOS 性能优化，增强 test 类，替换使用 objc2
- 修复 windows 读取 rtf 可能失败的情况
- 修复读取 html 少<的情况，【因为 wps 写入的 StartFragment 有问题】

### en

- macOS performance optimization, enhanced test class, replaced with objc2
- Fixed the case where reading rtf on windows may fail
- Fixed the case of reading html less than <, [because the StartFragment written by wps is problematic]

## v0.1.9 (2024-07-22) [released]

- Fix: Bug: `set` on windows without clear [issues#32](https://github.com/ChurchTao/clipboard-rs/issues/32)

## v0.1.8 (2024-07-18) [released]

- Fix: Bug: When read rimeout on Linux there is throw error but not
  loop [issues#30](https://github.com/ChurchTao/clipboard-rs/issues/30)

## v0.1.7 (2024-04-30) [released]

- Fix: Bug: Cannot write all content when writing to html on
  Windows [issues#23](https://github.com/ChurchTao/clipboard-rs/issues/23)

## v0.1.6 (2024-04-12) [released]

- Fix: Bug: Cannot paste after writing image to clipboard (on Windows)
  #17 [issues#17](https://github.com/ChurchTao/clipboard-rs/issues/17)
- Fix: Bug: No transparent background for clipboard image read on Windows
  #18 [issues#18](https://github.com/ChurchTao/clipboard-rs/issues/18)
- Fix: Bug: Cannot read clipboard image on MacOS for screenshots taken by certain apps
  #19 [issues#19](https://github.com/ChurchTao/clipboard-rs/issues/19)

## v0.1.5 (2024-04-11) [released]

- Fix: Fix the bug `fn get_image()` where image type is `CF_DIBV5`
  in `win11`. [issues#14](https://github.com/ChurchTao/clipboard-rs/issues/14)

## v0.1.4 (2024-03-18) [released]

- Fix: Fix the bug `fn read_files()` where no files in
  clipboard. [issues#11](https://github.com/ChurchTao/clipboard-rs/issues/11)

## v0.1.3 (2024-03-14) [released]

- Fix: Fix the bug on `Windows` can't read DIBV5 format image from
  clipboard [issues#8](https://github.com/ChurchTao/clipboard-rs/issues/8)
- Fix: Fix the bug on `Windows` can't move `WatcherContext` to another
  thread [issues#4](https://github.com/ChurchTao/clipboard-rs/issues/4)
- Change: Demo `watch_change.rs` The callback function for monitoring changes in the clipboard is changed to implement a
  trait. [pr#6](https://github.com/ChurchTao/clipboard-rs/pull/6)

## v0.1.2 (2024-03-08) [released]

- Change `rust-version = "1.75.0"` to `rust-version = "1.63.0"` [pr#3](https://github.com/ChurchTao/clipboard-rs/pull/3)
- Clean up the code and add some comments

## v0.1.1 (2024-03-04) [released]

- Feature: Add a option to `getFiles` or `setFiles`
- Feature: Add a option to `get` or `set` multi items
- Fix: make `WatcherShutdown` public
