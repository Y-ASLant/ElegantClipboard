# Features

Detailed feature list for ElegantClipboard.
For UI screenshots, see [README_EN.md](README_EN.md) (captured on v0.5.0 and may differ from the latest version).

## Terminology

- **Hover preview window** - The independent preview window shown on mouse hover (includes image and text previews)
- **Hover image preview** - Hover preview window for image content, supports `Ctrl+Scroll` zoom
- **Hover text preview** - Hover preview window for text/HTML/RTF content, supports `Ctrl+Scroll` scrolling

## Clipboard Management

- **Multi-type support** - Text, Image, File, HTML, RTF content types
- **Unlimited history** - Auto record all copied content, always accessible
- **Smart search** - Real-time search with optimized LIKE queries (CJK text optimized)
- **Content deduplication** - BLAKE3 hash auto-deduplication, no duplicate storage
- **Pin/Favorite** - Pin or favorite important items, immune to auto-cleanup
- **Drag sorting** - Drag to reorder, cross-zone drag auto-toggles status
- **Click to paste** - Click item to paste directly to active window
- **Paste as plain text** - Support paste as plain text (Shift+Enter or right-click menu)
- **Text editing** - Double-click or right-click to edit saved text
- **Source app recognition** - Auto record source application name and icon
- **Deduplication strategy** - Three modes: pin/ignore/always create new
- **Import/Export** - JSON format support for data migration

## Search Optimization

- **CJK compatibility** - Optimized LIKE queries, perfect support for Chinese and other CJK text
- **Smart field selection** - Only search preview and file path fields, avoid full table scan
- **Performance balance** - Best balance between accuracy and performance
- **Keyword highlight** - Auto extract keyword context during search for better UX

## Hover Image Preview

- **Thumbnail preview** - Auto generate thumbnails (Asset Protocol zero-overhead loading)
- **Single image file preview** - Copied image files display as image (fallback to file card on failure)
- **Hover preview window** - Show an independent preview window after configured hover delay (default 500ms), supports large image viewing
- **Ctrl+Scroll zoom** - Smooth zoom in the hover image preview window (CSS transition animation, zero window resize)
- **Zoom percentage badge** - Show percentage badge bottom-right, fade out after 1.2s
- **Preview position** - Auto/left/right three position preferences

## Hover Text Preview

- **Dedicated text preview window** - Text/HTML/RTF content can open in an independent hover preview window
- **Disabled by default** - Text preview is off by default and can be enabled in settings
- **Ctrl+Scroll for scrolling** - Reuses Ctrl+Scroll gesture to scroll text preview and avoid list-scroll conflicts
- **Theme and corner sync** - Automatically follows main window dark/light theme and sharp-corner mode
- **Preview position** - Same as image preview: auto/left/right

## File Management

- **File validity check** - Parallel check file existence (rayon), invalid files show red warning
- **Right-click menu** - Paste, paste as path, show in explorer, view details
- **File details dialog** - View full file info, mark invalid files

## Performance Optimization

- **Read/Write separation** - Database connection separation, reduce lock contention
- **WAL mode** - Enable WAL mode for concurrent read/write
- **Memory optimization** - Different cache sizes for write (64MB) and read (32MB) connections
- **Index optimization** - Partial indexes, composite indexes, descending indexes
- **Lock-free design** - Global mouse monitoring using atomic variables
- **Virtual scrolling** - react-virtuoso for efficient list, smooth with 10k+ items

## Window Management

- **Global shortcut** - Customizable shortcut to show/hide window (default Alt+C)
- **Win+V replacement** - Optional replace system Win+V (disable via registry)
- **Click outside to hide** - Global mouse monitoring, auto-hide on outside click (only when visible)
- **Window pin** - Lock window to prevent auto-hide
- **Follow cursor** - Optional show window at cursor position
- **Multi-monitor support** - Smart positioning, keep window within screen bounds
- **Remember window size** - Optional persist window size, restore on restart (default on)

## Customization

- **Toolbar customization** - Configure toolbar button visibility and order
- **Custom storage path** - Support data migration and custom path
- **History limit** - Set max records (0 for unlimited)
- **Content size limit** - Configurable max size per item
- **Display settings** - Preview lines (1-10), time format, char count/size/source app toggle
- **Card density** - Compact/Standard/Loose spacing
- **Sound feedback** - Optional copy/paste operation sounds
- **Preview settings** - Separate toggles for image/text preview, hover preview delay (default 500ms), zoom step (5%-50%), position preference
- **Window state reset** - Auto reset search and scroll on hide (optional)
- **Auto start** - Run on system startup
- **Admin launch** - Optional run as administrator (UAC elevation)
- **Database optimization** - Manual OPTIMIZE / VACUUM trigger
- **Data statistics** - Real-time database, image cache size and file count
- **Data cleanup** - Three levels: clear history / reset config / reset all data

## Appearance

- **System accent color** (default) - Auto read Windows system accent, real-time follow
- **Classic B&W** - Minimalist black/white/gray
- **Jade Green / Sky Cyan** - Preset color schemes
- **Dark mode** - Auto follow system dark/light mode
- **Window blur effect** - Mica / Acrylic / Tabbed Windows 11 DWM effects (Win10 fallback)

## Auto Update

- **Version check** - Auto check on startup, manual trigger in settings
- **Download progress** - Show download progress, cancelable
- **Changelog** - Display release notes

## System Integration

- **System tray** - Left-click toggle window, right-click menu (settings, restart, exit)
- **Non-focus window** - Window doesn't steal focus, no interruption
- **Keyboard simulation** - Windows SendInput, others use enigo for Ctrl+V
- **Quick paste** - Alt+number keys to quick paste items at position (customizable)
- **Startup notification** - Show system notification on launch with shortcut hints
- **Admin elevation** - UAC-free elevation via task scheduler (optional in settings)
