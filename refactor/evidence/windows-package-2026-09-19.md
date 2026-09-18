# Windows x64 独立包验收（2026-09-19）

- 在 Windows x64 MSVC 主机执行 `scripts/package-gpui-windows.ps1`，脚本用锁定依赖构建 Release，按 Cargo 包版本打包 exe、MIT 许可证与运行说明，并生成 SHA-256 文件。脚本拒绝其他构建目标；产物留在 Git 忽略的 `target/packages/`。
- 实际生成 `elegant-clipboard-gpui-v0.1.0-windows-x64.zip`，SHA-256 为 `9218312112be288856a46507f79a942f49f7a114698ab7ac642f45eae729ecc5`。独立读取归档核对三项文件、ZIP 校验与 SHA-256，解压到隔离目录后从包内 exe 运行 `--smoke-test --data-dir <隔离目录>`，退出码为 0，目标数据库创建成功。
- 此制品未签名、没有安装器和升级机制，不能视作 Windows 正式发布验收。常见应用之间的最终粘贴、中文 IME 与长时间常驻仍待验证。
