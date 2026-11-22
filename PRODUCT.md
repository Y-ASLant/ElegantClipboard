# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

Desktop-native (Windows, macOS, Linux), not mobile iOS/Android. Shared GPUI visual language across OSes; system integration (clipboard, hotkeys, tray, drag, paste delivery, window materials) is compiled per target. Windows is the primary acceptance platform; macOS and Linux compile in-tree with documented capability gaps (especially Wayland).

## Users

Primary: people who copy and paste across apps all day — writers, office work, research, development. Job: keep a searchable local history and put the right item back into the original window without breaking focus or format fidelity. Not positioned as a casual “occasional history” utility.

## Product Purpose

ElegantClipboard is a native clipboard manager (plus related local productivity surfaces such as memos/todos in the same app). It captures clipboard content, lets the user find it later, and pastes or copies it back with high fidelity. Success means: capture what the OS actually held, retrieve it fast, return it to the prior app without stealing focus or silently degrading formats, and keep data local and under the user’s control.

## Positioning

Claims neighboring clipboard tools cannot truthfully copy as a set:

- **Native format fidelity**: beyond text/URL/HTML/RTF/image/file paths, other native formats (including app-registered ones) are captured under a built-in blocklist and written back with the main format; large payloads use content-addressed blob storage.
- **Controlled paste-back**: history UI can avoid activating over the source app; paste returns to the original window under explicit request/generation rules.
- **True native stack**: Rust + GPUI + GPUI Kit only — no Node, WebView, or Tauri on this branch.
- **Local privacy controls**: on-device SQLite; source-app filtering; optional transient-fragment filter; pause capture; no fabricated cloud or telemetry claims.
- **Workflow depth**: sequence paste, same-text merge across rich formats, groups/favorites/pins, and in-app memo/todo alongside clipboard history.
- **Cross-platform parity intent**: one shared UI and data service; per-OS integration with documented limits, not three unrelated products.

## Operating Context

- Always-available desktop utility: global hotkey, tray, optional autostart, single-instance re-activation.
- Typical scene: user is mid-edit in another app → summons history → search/select → paste back or drag out.
- First run: three-step onboarding in the main panel (language, shortcuts practice on isolated demo data, theme/autostart preferences).
- Settings domains: general, display, appearance, data, app filter, sound, shortcuts, about.
- Data lives in OS-standard app data dirs; backups are GPUI ZIP; legacy 1.2.10 ZIP import into an empty directory is supported. Old data dirs and autostart entries are not auto-migrated.
- Windows-only: optional relaunch elevated (UAC) to interact with elevated target apps.

## Capabilities and Constraints

**Capabilities (shipped product truth)**

- Capture/search: text, URL, HTML/RTF, images, file paths; notes editable and searchable.
- Dedup/merge: repeated plain text can bump existing entry; HTML/RTF of same text can merge; optional per-copy rows; ~300 ms transient confirmation window (configurable).
- Copy/paste modes: original multi-format write-back; single-format rich copy/paste with explicit error if missing (no silent downgrade); plain-text transforms; optional restore of prior clipboard after paste (off by default).
- Sequence paste via global advance key; queue cancels on leave/copy/delete.
- Hover preview: non-activating, mouse-passthrough layer; HTML via markdown rendering path; Ctrl+wheel zoom/scroll.
- Groups, favorites, pins, drag reorder, batch delete/cleanup; retention by count/days protecting favorites/pins.
- Quick-paste slots (off by default); copy/paste sounds; bilingual UI; light/dark; corner radius; platform materials (mica/blur/opaque as available).
- Memos/todos coexist in the app (shared drag/preview patterns with history where implemented).

**Hard technical constraints**

- Stack: Rust workspace `clipboard-gpui` → `clipboard-platform` → `clipboard-core`; GPUI Kit for catalog controls; no second UI framework.
- Per-file Rust sources ≤ 640 physical lines (560+ warns; split by cohesive responsibility, not LOC targets; `scripts/check-source-size.ps1` / `just check`). Process detail: `docs/README.md`.
- Content limits: text/rich ≤ 1 MiB combined rules as implemented; images ≤ 25M pixels / 50 MiB PNG; file lists path-only ≤ 256 items or 1 MiB; no staged file copies.
- Wayland: requires compositor data-control; no silent XWayland capture fallback; global hotkey/auto-paste/source/position limited or unavailable.
- Known product limits: remote http(s) images in HTML preview not loaded; editing HTML saves as plain text; some apps (e.g. WeChat) delay-render image formats with no data — capture skipped silently.
- Acceptance: native interaction verified primarily on Windows; macOS/Linux still need on-host acceptance for desktop interaction, permissions, and packages.
- No auto-update. Installers do not delete history on uninstall.

**Open / undecided**

- Formal accessibility standard level beyond system reduce-motion + keyboard reachability (not WCAG-committed).
- Depth of non-Windows native acceptance as a release gate remains process judgment, not a fixed product SLA in-repo.

## Brand Commitments

- Name: **ElegantClipboard** (fixed).
- UI languages: Simplified Chinese and English.
- License: MIT; no separate commercial brand book or marketing voice guide on hand.
- Implementation UI rules live in `docs/UI_GUIDELINES.md` and `design_system.rs` (Chakra-inspired tokens/motion as engineering convention). Those are incumbent implementation constraints for refinement work, not a separate marketing brand system invented here.

## Evidence on Hand

- Product/feature truth: `README.md`, `README_EN.md`
- Architecture and module map: `docs/README.md`, `docs/WINDOWS_MVP.md`
- UI implementation rules: `docs/UI_GUIDELINES.md`
- ADRs: `docs/decisions/0001`–`0004` (GPUI choice, native fidelity/transient merge, paste parity, sequence paste)
- Verification notes: `docs/evidence/` (Windows-heavy; treat as historical acceptance, not marketing proof)
- Do **not** fabricate: testimonials, benchmarks, customer logos, pricing, cloud sync, telemetry dashboards, or cross-OS “fully equal” capability claims.

## Product Principles

1. **Fidelity over convenience defaults** — never silently drop or downgrade clipboard formats the user asked to preserve; fail explicitly when a chosen format is missing.
2. **Stay out of the way of the source app** — summon, select, paste back; protect focus, caret, and paste-target validity.
3. **Local-first and inspectable** — data on disk the user controls; filtering, pause, retention, and backup are first-class; no implied cloud.
4. **One product, honest platform limits** — shared workflows everywhere; document and surface OS gaps instead of fake parity.
5. **Operate-mode craft** — scanability, keyboard paths, and native OS integration beat decorative expression; density and latency matter under real history sizes.

## Accessibility & Inclusion

Follow OS reduce-motion; keep keyboard navigation for core list/search/paste flows; preserve focus affordances without heavy custom focus rings where Kit defaults apply. No formal WCAG 2.x AA (or other) target is committed. Screen-reader depth is bounded by GPUI/Kit capabilities and must not be advertised beyond what is implemented.
