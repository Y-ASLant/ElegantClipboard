SHELL := powershell.exe
.SHELLFLAGS := -NoProfile -Command
.DEFAULT_GOAL := help

.PHONY: help clean build run check format test

help:
	@Write-Host "Usage: make <target>"
	@Write-Host ""
	@Write-Host "Available targets:"
	@Write-Host "  make clean   Remove frontend dist and Rust target artifacts"
	@Write-Host "  make build   Build Tauri release installer"
	@Write-Host "  make run     Build frontend and run backend"
	@Write-Host "  make check   Run lint, TypeScript, and cargo check"
	@Write-Host "  make test    Run frontend unit tests, component tests, and perf benchmarks"
	@Write-Host "  make format  Run frontend autofix and Rust formatter"

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
	@Write-Host "[check] typescript"
	npx tsc --noEmit
	@Write-Host "[check] cargo"
	cargo check --manifest-path src-tauri/Cargo.toml
	@Write-Host "[check] done"

test:
	@Write-Host "[test] unit tests, component tests, and perf benchmarks"
	npx vitest run --reporter=verbose
	@Write-Host "[test] done"

format:
	@Write-Host "[format] eslint autofix"
	npm run lint:fix
	@Write-Host "[format] rustfmt"
	@Set-Location src-tauri; Get-ChildItem -Path src -Recurse -Filter *.rs | ForEach-Object { rustup run stable rustfmt --edition 2024 $$_.FullName }
	@Write-Host "[format] done"
