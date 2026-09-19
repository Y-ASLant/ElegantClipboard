# Windows 历史网址兼容验证

- 旧版将合法网址归类为 `url`，使用去除首尾空白后的 `url:` BLAKE3 哈希。GPUI 历史列表和计数现同时查询 `text,url`，打开、复制、搜索、置顶、收藏、拖拽复用文本操作。
- GPUI 新采集网址使用相同分类与哈希，避免导入旧库后再次复制相同网址产生重复项。其他文本仍保留原字节和精确去重规则。
- 回归单测模拟旧版 `url` 记录，验证搜索可见、原 ID 复用、首尾空白归一化及全文读取。
- `cargo test --workspace --locked --quiet`：100 项通过；Clippy `-D warnings`、fmt、Windows release 构建与隔离目录的 release 窗口冒烟均通过。
