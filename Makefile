SHELL := powershell.exe
.SHELLFLAGS := -NoProfile -Command
.DEFAULT_GOAL := help

.PHONY: help clean build run check format test

RUST_SYSROOT := $(shell rustc --print sysroot)
RUST_ENV = $$env:Path = "$(RUST_SYSROOT)\bin;" + $$env:Path;

help:
	@Write-Host "Usage: make <target>"
	@Write-Host ""
	@Write-Host "Available targets:"
	@Write-Host "  make clean   Remove frontend dist and Rust target artifacts"
	@Write-Host "  make build   Build Tauri release installer"
	@Write-Host "  make run     Build frontend and run backend"
	@Write-Host "  make check   Frontend types/lint and app/fork fmt/check/Clippy (zero warnings)"
	@Write-Host "  make test    Frontend, backend, and fork pure tests (default/no-default features)"
	@Write-Host "  make format  Frontend import autofix and app/fork Rust formatting"

clean:
	@Write-Host "[clean] remove frontend dist"
	if (Test-Path dist) { Remove-Item -Recurse -Force dist }
	@Write-Host "[clean] remove Vite cache"
	if (Test-Path node_modules/.vite) { Remove-Item -Recurse -Force node_modules/.vite }
	@Write-Host "[clean] remove Rust target"
	cargo clean --manifest-path src-tauri/Cargo.toml
	@Write-Host "[clean] done"

build:
	@Write-Host "[build] tauri release installer"
	npm run tauri build
	@Write-Host "[build] done"

run:
	@Write-Host "[run] frontend production build"
	npm run build
	@Write-Host "[run] start backend"
	cargo run --manifest-path src-tauri/Cargo.toml

check:
	@Write-Host "[check] eslint"
	npm run lint
	@Write-Host "[check] application, test, and configuration TypeScript"
	npm run typecheck
	@Write-Host "[check] Rust formatting"
	$(RUST_ENV) cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
	$(RUST_ENV) cargo fmt --manifest-path clipboard-rs/Cargo.toml --all -- --check
	@Write-Host "[check] cargo"
	cargo check --manifest-path src-tauri/Cargo.toml --all-targets
	@Write-Host "[check] Clippy (zero warning gate)"
	$(RUST_ENV) cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
	$(RUST_ENV) cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets -- -D warnings
	$(RUST_ENV) cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features -- -D warnings
	@Write-Host "[check] done"

test:
	@Write-Host "[test] frontend behavioral regressions"
	npm test
	@Write-Host "[test] backend"
	cargo test --manifest-path src-tauri/Cargo.toml --all-targets
	@Write-Host "[test] clipboard fork pure tests (native clipboard tests are explicit opt-in)"
	cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets
	cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features
	@Write-Host "[test] done"

format:
	@Write-Host "[format] eslint autofix"
	npm run lint:fix
	@Write-Host "[format] rustfmt"
	$(RUST_ENV) cargo fmt --manifest-path src-tauri/Cargo.toml --all
	$(RUST_ENV) cargo fmt --manifest-path clipboard-rs/Cargo.toml --all
	@Write-Host "[format] done"
