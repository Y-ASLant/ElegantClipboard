# Windows x64 独立包验收（2026-09-19）

- 在 Windows x64 MSVC 主机执行 `scripts/package-gpui-windows.ps1`，脚本用锁定依赖构建 Release，按 Cargo 包版本打包 exe、MIT 许可证与运行说明，并生成 SHA-256 文件。脚本拒绝其他构建目标；产物留在 Git 忽略的 `target/packages/`。
- 使用 gpui-kit 0.6.2 时生成的先前 ZIP 摘要为 `84596713359b09de7024a644f4a2b473af3a386942ff9bab18115533099e0d74`。当时独立读取归档核对 exe、许可证、`README.txt` 三项文件，包内 exe 与 Release 构建一致，说明与工作区原文一致；实测摘要与 `.sha256` 一致。解压到隔离目录 `target/qa/package-20260919-target-lifecycle` 后从包内 exe 运行 `--smoke-test --no-monitor --data-dir <隔离目录>`，退出码为 0。
- 更新到 gpui-kit 0.6.4 后生成的先前同名 ZIP 摘要为 `86d43178a591e086eb030a9aedf7ad7cb4c1985f3a350c38a4c3310bbaa21b56`；当时从包内 exe 运行隔离 `--smoke-test`，退出码为 0。
- 修正旧库导入提示后重新打包的先前同名 ZIP 摘要为 `729457db12e178bd3d3fc7d5dab57e001cf538bdefabe618e2ea43f0bd1334cb`。当时解压到隔离目录 `target/qa/package-current-1724062e632747e1b524e90c29c7ed6d/`，从包内 exe 导入两条合成历史，提示正确，窗口冒烟退出码为 0。
- 富文本多格式写回初次修复后的先前同名 ZIP 摘要为 `4b364162e2747ce63bbf2a35e7c4e7e89a2cbce20443de4dfb014a582d6d83c1`；当时解压到 `target/qa/rich-package-a95223931b75412a92a60588a3ba50bd/` 后窗口冒烟退出码为 0。
- 完善写入序号与读回稳定性检查后的先前同名 ZIP 摘要为 `8ee041c4e8b88dcf894ae697bcc70cdf8eebe33dd5b6fd141eda0422cd59dda2`；当时解压到 `target/qa/rich-package-final-f307b8481c2a40fe803f672f46ca564a/` 后窗口冒烟退出码为 0。
- 收紧部分写入失败的序号抑制后的先前同名 ZIP 摘要为 `d0ebfd7970f9748f7d0550f9d3746224ec8681967bf527f8539750ea0d9ed87c`；当时解压到 `target/qa/rich-package-release-0bd58f83f8fd4668b7fea9e32d8dd48e/` 后窗口冒烟退出码为 0。
- 增加历史列表首尾与翻页键后的先前同名 ZIP 摘要为 `018554df830e2c5cfa5e9426e04b91d74f660ab3accfcea1f718655ec9179552`；当时解压到 `target/qa/keyboard-package-15e1bee658ee432b81f59003e66e4ad0/` 后窗口冒烟退出码为 0。
- 增加 Ctrl+A 全选和 Esc 清除批量选择后的先前同名 ZIP 摘要为 `dee330c6361dd511bfc66ffd35e5b33ac77e6910f04beef17cc223db999e9cfc`；当时解压到 `target/qa/batch-keyboard-package-0f507529a34c4b548192277f6a051b0a/` 后窗口冒烟退出码为 0。
- 增加批量合并复制与受控粘贴后生成的先前同名 ZIP 摘要为 `6b882cf29d1559fb143e1263594aac18d40c5dd29bb48bc735b50f0d00736a50`；当时解压到 `target/qa/merge-package-final-eb79716880924d0baa8c7a983d38b870/` 后以独立数据目录运行 `--smoke-test`，退出码为 0。
- 增加文件与图片路径复制、受控粘贴和资源管理器定位后生成的先前同名 ZIP 摘要为 `9675251038fe388d3c08bb66a5bd8345b30ca7b8c04d4e7e791c854232178647`；当时解压到 `target/qa/file-actions-package-6087b94ed1824632a22a150f5775080a/` 后以独立数据目录运行 `--smoke-test`，退出码为 0。
- 增加文件与图片另存为后生成的先前同名 ZIP 摘要为 `3c866bba97e5a635737aac609b017ef9eb2602517c64e0c72e4397ccbb886def`；当时解压到 `target/qa/save-as-package-26b4dedb3a144ea19a7f08d426960ce8/` 后以独立数据目录运行 `--smoke-test`，退出码为 0。
- 增加数据占用统计与数据目录入口后再次构建 Release，当前 ZIP 的 SHA-256 为 `80d40f120ef51745d0b4b95b74ba14c65613cbedb5e669acd5a41e43ca10c0b7`，与 `.sha256` 相符。包内 exe 与 Release exe 的 SHA-256 均为 `241193ac6f2c4a4cfd56596c0ce513da1d2a3e82aaf874188b13eac6b8b0f550`，归档仅含 exe、许可证和运行说明；解压到 `target/qa/data-size-package-final-507c6fce7d1f41b498c7ef23a9fa7e0b/` 后以独立数据目录运行 `--smoke-test`，退出码为 0。以上先前摘要只用于追溯，已不对应当前同名 ZIP。
- 此制品未签名、没有安装器和升级机制，不能视作 Windows 正式发布验收。搜索、分组创建和正文编辑的拼音输入已在活动桌面验收；其他输入法及输入场景、常见应用之间的最终粘贴与长时间常驻仍待验证。
