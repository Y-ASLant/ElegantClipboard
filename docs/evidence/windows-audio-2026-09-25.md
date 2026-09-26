# GPUI 复制与粘贴音效验证

- 对照主分支音效页，在 GPUI 设置导航中加入复制和受控粘贴两张设置卡：独立开关、立即/成功后时机、无视开关的试听按钮。原生播放使用进程内生成的短 PCM WAVE；异步播放缓冲区在进程生命周期内保持有效。
- 在 `target/ui-audio-20260925` 隔离数据目录用 `--no-monitor` 启动调试版，进入音效页，逐项启用两组音效并设为“成功后”。SQLite 中 `gpui_audio` 为 `{"copy_enabled":true,"copy_timing":"after_success","paste_enabled":true,"paste_timing":"after_success"}`。点击复制试听后未见音频设备错误；未录制声学输出，因此不宣称扬声器播放已被独立测量。
- 单元测试覆盖默认关闭、两种时机选择、WAVE 格式及首部长度；平台测试覆盖保存事件和重启加载，备份测试覆盖恢复设置。`--no-monitor` 测试没有写入用户剪贴板；真实跨应用复制和受控粘贴音效仍需活动桌面验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 与 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；224 项测试通过、2 项忽略。发布 EXE 的 PE 子系统为 Windows GUI（2），隔离目录下 `--smoke-test` 退出码为 0。
