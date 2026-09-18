# Windows x64 独立包验收（2026-09-19）

- 在 Windows x64 MSVC 主机执行 `scripts/package-gpui-windows.ps1`，脚本用锁定依赖构建 Release，按 Cargo 包版本打包 exe、MIT 许可证与运行说明，并生成 SHA-256 文件。脚本拒绝其他构建目标；产物留在 Git 忽略的 `target/packages/`。
- 使用 gpui-kit 0.6.2 时生成的先前 ZIP 摘要为 `84596713359b09de7024a644f4a2b473af3a386942ff9bab18115533099e0d74`。当时独立读取归档核对 exe、许可证、`README.txt` 三项文件，包内 exe 与 Release 构建一致，说明与工作区原文一致；实测摘要与 `.sha256` 一致。解压到隔离目录 `target/qa/package-20260919-target-lifecycle` 后从包内 exe 运行 `--smoke-test --no-monitor --data-dir <隔离目录>`，退出码为 0。
- 更新到 gpui-kit 0.6.4 后重新生成同名 ZIP，当前 SHA-256 为 `86d43178a591e086eb030a9aedf7ad7cb4c1985f3a350c38a4c3310bbaa21b56`，与 `.sha256` 相符。解压到 `target/qa/gpui-kit-064-package-20260919/` 后从包内 exe 运行隔离 `--smoke-test`，退出码为 0。旧摘要只用于追溯先前构建，已不对应当前同名 ZIP。
- 此制品未签名、没有安装器和升级机制，不能视作 Windows 正式发布验收。搜索、分组创建和正文编辑的拼音输入已在活动桌面验收；其他输入法及输入场景、常见应用之间的最终粘贴与长时间常驻仍待验证。
