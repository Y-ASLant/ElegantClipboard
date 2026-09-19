SHELL := powershell.exe
.SHELLFLAGS := -NoProfile -Command
.DEFAULT_GOAL := help

.PHONY: help clean build package run check test format

help:
	@Write-Host "Usage: make <target>"
	@Write-Host ""
	@Write-Host "Available targets:"
	@Write-Host "  make clean    Remove Cargo build artifacts"
	@Write-Host "  make build    Build the GPUI release executable"
	@Write-Host "  make package  Build the Windows x64 GPUI ZIP package"
	@Write-Host "  make run      Run the GPUI application"
	@Write-Host "  make check    Run rustfmt, cargo check, and Clippy"
	@Write-Host "  make test     Run all workspace tests"
	@Write-Host "  make format   Format the Rust workspace"

clean:
	cargo clean

build:
	cargo build -p elegant-clipboard-gpui --release --locked

package:
	pwsh.exe -NoProfile -File scripts/package-gpui-windows.ps1

run:
	cargo run -p elegant-clipboard-gpui --locked

check:
	cargo fmt --all -- --check
	cargo check --workspace --all-targets --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings

test:
	cargo test --workspace --locked

format:
	cargo fmt --all
