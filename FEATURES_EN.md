# Features

Main features of the current `main` branch. Defaults refer to fresh settings; saved preferences may differ.
For UI screenshots, see [README_EN.md](README_EN.md) (captured on v0.5.0 and may differ from the latest version).

## Terminology

- **Hover preview window** - The independent preview window shown on mouse hover (includes image and text previews)
- **Hover image preview** - Independent image preview, zoomed with `Ctrl+Scroll` in the main window
- **Hover text preview** - Independent preview for text/URL/HTML/RTF, scrolled with `Ctrl+Scroll` in the main window

## Clipboard Management

- **Multi-type support** - Text, Image, File, URL (link), HTML, RTF
- **Custom groups** - Per-item `group_id` assignment; drag ordering within groups with pinned items first; group views show assigned items, movable via the context menu
- **History recording** - Background capture subject to type, size, and source filters; the default limit is 10,000 entries. Set the count limit to 0 for unlimited count; age-based cleanup is configured separately
- **Smart search** - Search full text and file paths with LIKE substring queries, including CJK text
- **Content deduplication** - BLAKE3 content hashes and text semantic hashes support per-group deduplication; “always create new” can retain duplicates
- **Pin/Favorite** - Pin or favorite important items, immune to auto-cleanup
- **Drag sorting** - Drag using the handles on either side of a card; crossing pinned/unpinned zones toggles its pinned state
- **Click to paste** - Click item to paste directly to active window
- **Paste as plain text** - Shift+Enter with keyboard navigation enabled, or the context menu
- **Text editing** - Edit saved text, URL, and rich-text entries in a dedicated editor via the context menu
- **Source app recognition** - Auto record source application name and icon
- **Deduplication strategy** - Move to top (default), ignore, or always create new; semantic (default) or strict text matching. Moving to top does not permanently pin the item
- **Import/Export** - ZIP backup and restore of the database and app-managed images, icons, and staged files (Settings → Data); not a backup of every original file

## Operation Checks and Feedback

- **Unified preflight** - Cards, toolbars, context menus, Enter/Shift+Enter, global quick/favorite/repeat paste, merged paste, and copying/pasting direct text or translation results all use backend checks of the actual content and required resources before execution. UI availability is an early indication, not a substitute for execution-time checks
- **Check before changing state** - Failed preflight leaves the existing clipboard, main-window visibility, and monitor pause state unchanged. Merged paste checks every item first instead of silently omitting an invalid item and proceeding. Actual clipboard writes or simulated input can still fail; passing preflight does not mean the operation has completed
- **Success / cancelled / failed** - Only backend success triggers success animation, a copy-completed indicator, post-operation selection changes, or configured ordering updates; failures are not treated as successes. Dismissing the Save As dialog is cancellation, with no success or error message. If the operation completes but a subsequent list refresh fails, refresh failure is reported separately without changing the completed operation to failure
- **Safe error messages** - The interface language is used for the operation name and controlled reasons, such as a deleted item, missing/unreadable resource, denied permission, invalid/unsupported content, image decode failure, a busy clipboard, or failed paste. Unknown errors use a generic message; raw diagnostics are logged rather than displaying paths or raw exceptions. Background logging and user feedback are handled separately to avoid duplicate popups
- **Global shortcut feedback** - Failures appear in the main window when it is visible, or through a system notification when hidden. Failed simulated paste restores a previously visible main window without stealing focus or changing its pinned state. Success means clipboard writing and input simulation succeeded, not that the target application accepted or used the content

## WebDAV Sync

- **Self-hosted sync** - Enable WebDAV in Settings → Plugins, then configure the server and sync; disabled by default
- **Auto sync** - Background upload at a configurable interval after separately enabling auto sync (default interval: 60s)
- **Manual sync** - Manually trigger sync operations
- **Selective sync** - Independent toggles for text, image, file, video sync
- **Media sync** - Images and files synced via independent media mapping, supports incremental upload
- **Proxy support** - System proxy / custom proxy / no proxy modes
- **Self-signed certificates** - Option to accept invalid TLS certificates (off by default; enabling it weakens connection security)

## Internationalization

- **Three languages** - Simplified Chinese (default), English, Traditional Chinese
- **Settings** - General → Interface language
- **Multi-window sync** - `locale-changed` event syncs main and settings windows
- **Developer guide** - All UI strings via `t()` in `src/i18n/`; see `src/i18n/README.md`


## Translation

- **Clipboard translation** - Enable Translation in Settings → Plugins and configure a service; supports multiple languages. Online services receive the text being translated
- **Dedicated translation window** - Translation results in an independent window, no interference with main window
- **Translation settings** - Configure source language, target language, and translation service

## Search Optimization

- **CJK compatibility** - LIKE substring matching includes Chinese and other CJK text without a tokenizing full-text index
- **Search fields** - Match full `text_content` and `file_paths`, not just the card preview
- **List transfer** - Browsing omits full text, HTML, RTF, and `file_payload`; search reads full text to generate context, then omits it from the response
- **Keyword highlight** - Auto extract keyword context during search for better UX

## Hover Image Preview

- **Card image preview** - Load cached images through Tauri Asset Protocol and display at card size (no separate thumbnail files generated)
- **Single image file preview** - Available single image files below the preview size threshold show an image preview; load failures or oversized files use a file card
- **Hover preview window** - Independent preview after the configured delay (current default: 128ms); image preview is enabled by default
- **Ctrl+Scroll zoom** - Zoom hover image preview from the main window, with smooth content-size animation and an optional unbounded preview mode
- **Zoom percentage badge** - Show percentage badge bottom-right, fade out after 1.2s
- **Preview position** - Auto/left/right three position preferences

## Hover Text Preview

- **Dedicated text preview window** - Text/URL/HTML/RTF can open in an independent hover preview window
- **Enabled by default** - Text preview is on by default and can be disabled separately
- **Ctrl+Scroll for scrolling** - Scroll text preview from the main window without scrolling the list
- **Theme and corner sync** - Automatically follows main window dark/light theme and sharp-corner mode
- **Preview position** - Same as image preview: auto/left/right

## File Management

- **Original paths and staged fallback** - File history stores the original copied paths and may stage readable regular files within the staging size limit. After an original is moved or deleted, only a still-available staged copy can provide fallback. Directories, oversized files, and failed staging are not guaranteed recoverable; new locations are not tracked automatically
- **High-fidelity file restore** - Capture Windows CF_HDROP and supported companion formats into `file_payload`; validate the payload and resources before execution, restoring the original context when available and using staged paths when needed. Supported complete virtual-file payloads with descriptors and nonempty `FileContents` can pass validation, but capture does not save `FileContents` and does not guarantee general virtual-file recovery. Descriptor-only, missing-content, or zero-byte-content virtual payloads cannot be copied/pasted. Zero-byte raw-format publication on Windows is explicitly rejected before changing the clipboard
- **Merge paste** - Merge selected items into the clipboard (joined text + combined file paths) using the same preflight checks. Incompatible file-payload combinations fail explicitly instead of silently dropping formats or items
- **Unified resource status** - Visible file and cached-image cards share unknown / checking / available / unavailable states. Original/staged paths and cached-image paths are checked in separate batches on display, window reopening, and context-menu opening, without polling or reading file contents. Text and URLs do not trigger disk or network checks
- **Stable display during rechecks** - Results for up to 512 recently checked resource sources are kept in session memory, not the database. Virtualized remounts, window reopening, and context-menu rechecks retain the previous label and preview paths until a new result arrives; uncached sources start in “checking”. Content, path, or size changes do not reuse the old source's result. Resource copy/paste, Save As, and Explorer actions remain disabled during rechecks. Failed checks discard the cache and show unknown rather than authorizing actions from an old label. Remounts still perform batched checks; this does not promise fewer disk checks
- **Availability, preview, and execution are separate** - Missing files or cached images show a warning and red invalid marker; single-file paths are struck through. Image decode/load errors only show a preview-load failure. Size limits only skip image loading; neither preview failure nor size alone marks a resource unavailable. “Available” does not guarantee continued readability or decodable content: execution checks these separately. Copy/paste requiring image data must decode it, but a readable original can still be saved as raw bytes even if its preview fails
- **Resource existence and clipboard capability are separate** - Card clicks, the copy toolbar, and context menus check both resource existence and whether the payload can be published to the clipboard. Unknown, checking, unavailable resources or unsupported payloads disable resource copy/paste. An unsupported companion format does not mark an existing physical file missing: with an available disk path, it can still be saved as raw bytes or located in Explorer. A usable virtual clipboard payload does not imply a disk path for Save As or Explorer. Keyboard actions, global shortcuts, merges, and paste as path also receive the appropriate backend checks. Usable staged copies can be used for operations; delete, favorite, and grouping do not depend on resource availability or preview results
- **Fresh item-based resource lookup** - Save As and Show in Explorer reload the current history entry and resolve original/staged resources rather than trusting an old path cached by a card. Save As rechecks the source after a destination is chosen. Deleted entries or sources that are no longer available or readable fail explicitly
- **File details dialog** - Show names, paths, file/directory type, and validity; opening it is subject to the availability check

## Performance Optimization

- **Read/Write separation** - Database connection separation, reduce lock contention
- **WAL mode** - Enable WAL mode for concurrent read/write
- **Connection caches** - SQLite page-cache limits of approximately 64,000 KiB for writes and 32,000 KiB for reads; not a fixed memory-usage guarantee
- **Index optimization** - Partial indexes, composite indexes, descending indexes
- **Atomic state** - Global mouse monitoring tracks state and cursor position with atomic variables
- **Virtual scrolling** - List layout uses react-virtuoso to render visible items and a buffer; an optional masonry layout is also available

## Window Management

- **Global shortcut** - Customizable shortcut to show/hide window (default Alt+C)
- **Win+V replacement** - Optional replace system Win+V (disable via registry)
- **Click outside to hide** - A global mouse hook handles outside-click hiding only while the main window is visible; separate from background clipboard watching
- **Window pin** - Lock window to prevent auto-hide
- **Follow cursor** - Optional show window at cursor position
- **Multi-monitor support** - Smart positioning, keep window within screen bounds
- **Remember window size** - Optional persist window size, restore on restart (default on)

## Customization

- **Interface language** - Simplified Chinese / English / Traditional Chinese
- **Toolbar customization** - Configure toolbar button visibility and order (incl. WebDAV sync shortcut)
- **Custom storage path** - Support data migration and custom path
- **History limit** - Configurable maximum count (default 10,000; 0 removes the count limit)
- **Auto cleanup** - Configurable age (default 30 days; 0 disables it). Saving a new entry cleans expired ordinary entries in its group; pinned/favorite entries are protected
- **Size limits** - Configurable text-like content capture limit (default 1,024 KiB) and image-size/file-image-preview threshold (default 51,200 KiB). File-path entries are not discarded just because their image preview is too large
- **App source filtering** - Blacklist/whitelist mode, wildcard matching for app name, process name, process path
- **Display settings** - Preview lines (1-10), time format, char count/size/source app toggle
- **Card density** - Compact/Standard/Loose spacing
- **Sound feedback** - Optional copy/paste operation sounds
- **Preview settings** - Independent image/text toggles (both on by default), hover delay (default 128ms), zoom step (5%-50%, default 15%), position preference, and optional unbounded image preview
- **Window state reset** - Reset search and scroll on hide (off by default); clearing search on reopening is an independent option (on by default)
- **Auto start** - Current-user registry Run entry via tauri-plugin-autostart; autostart and administrator preference are separate settings
- **Admin launch** - Preference stored as `run_as_admin` in `config.json`; startup prefers a valid existing elevation task, requesting UAC when needed
- **Database optimization** - Manual OPTIMIZE / VACUUM trigger
- **Data statistics** - View and manually refresh sizes/counts for the database, images, icons, and staged files
- **Data cleanup** - Three levels: clear history / reset config / reset all data

## Appearance

- **System accent color** (default) - Auto read Windows system accent, real-time follow
- **Classic B&W** - Minimalist black/white/gray
- **Jade Green / Sky Cyan** - Preset color schemes
- **Light/dark mode** - Light / dark / automatic (follow system by default)
- **Window backdrop effects** - none / Mica / Acrylic / Tabbed; support depends on Windows version. Startup application failures use a no-effect appearance; a failed settings change restores the previous choice

## Auto Update

- **Version check** - Automatic startup checks are enabled by default (can be disabled); manual checks from settings or the tray menu
- **Download progress** - Show download progress, cancelable
- **Changelog** - Display release notes
- **System proxy support** - Update checks and downloads read Windows system proxy configuration; connectivity still depends on the network and proxy

## System Integration

- **System tray** - Left-click toggles the window; menu includes pause/resume recording, disable/restore shortcuts, settings, update checks, restart, and exit. The tray icon can optionally be hidden
- **Non-focus window** - The main window does not steal focus by default; search, settings, and editing can acquire focus as needed
- **Keyboard simulation** - Windows SendInput simulates Ctrl+V or Shift+Insert (configurable); current builds and releases target Windows
- **Quick paste** - Ten positions in the current list default to Alt+1…9/0, individually customizable or disableable
- **Favorite paste** - Ten independently configurable shortcuts paste corresponding positions in the favorites list; only the first three default to Ctrl+Alt+1/2/3. These are not dedicated slots bound permanently to particular entries
- **Startup notification** - Optional startup notification with the active show/hide shortcut
- **On-demand elevation** - An elevated process can create a scheduled task for later launches; if unavailable, UAC is used. A prompt-free first launch is not guaranteed
- **Portable mode** - Standalone x64 / arm64 executables; portable mode is detected by the absence of `uninstall.exe` beside the exe. Writable directories and WebView2 Runtime are required
