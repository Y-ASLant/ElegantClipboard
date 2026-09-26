# GPUI 文件卡片来源状态验证

- 从历史快照读取文件路径，独立后台线程检查每项元数据。任一原始来源不存在时卡片显示“源文件已失效”，元数据不可读或路径无效时显示“文件无法读取”。提示不判断旧备份暂存副本能否用于复制；执行复制仍由原有后台流程重新解析并校验。
- 使用 `target/ui-expanded-image-20260925` 隔离目录与调试版 `--no-monitor`。合成 `missing-file.png` 记录在卡片标题显示红色“源文件已失效”，文件名也变红；同屏的有效 `sample.png` 继续显示缩略图，没有被误标。未启用剪贴板采集，也未执行复制或粘贴。
- 单元测试覆盖单文件存在、缺失、目录、超大图片、多文件中部分缺失及无效路径数据。真实网络路径、权限拒绝、受管暂存副本和外部文件变化后的重新检查仍需验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build -p elegant-clipboard-gpui --release --locked` 均通过；230 项测试通过、2 项按配置忽略。发布版隔离窗口确认失效来源红色提示与有效 PNG 缩略图同时显示；发布 EXE 的 PE 子系统为 Windows GUI（2），隔离 `--smoke-test` 退出码为 0。
