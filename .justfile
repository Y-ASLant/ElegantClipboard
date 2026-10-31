# ElegantClipboard 任务快捷方式，替代原 Makefile
# 运行 `just` 或 `just --list` 查看全部任务

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# 显示可用任务
default:
    @just --list

# 删除 Cargo 构建产物
clean:
    cargo clean

# 构建 GPUI release 可执行文件
build:
    cargo build -p elegant-clipboard-gpui --release --locked

# 构建 Windows x64 GPUI ZIP 包
package:
    pwsh.exe -NoProfile -File scripts/package-gpui-windows.ps1

# 在 macOS/Linux 本机生成原生包（PowerShell 7；Linux 安装包还需要 dpkg-dev 和 rpm）
[unix]
package-unix:
    pwsh -NoProfile -File scripts/package-gpui-unix.ps1

# 运行 GPUI 应用
run:
    cargo run -p elegant-clipboard-gpui --locked

# 检查模块行数、rustfmt、cargo check 与 Clippy
[windows]
check:
    powershell.exe -NoProfile -File scripts/check-source-size.ps1
    cargo fmt --all -- --check
    cargo check --workspace --all-targets --locked
    cargo clippy --workspace --all-targets --locked -- -D warnings

[unix]
check:
    pwsh -NoProfile -File scripts/check-source-size.ps1
    cargo fmt --all -- --check
    cargo check --workspace --all-targets --locked
    cargo clippy --workspace --all-targets --locked -- -D warnings

# 运行所有 workspace 测试
test:
    cargo test --workspace --locked

# 格式化 Rust workspace
format:
    cargo fmt --all
