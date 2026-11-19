---
name: ElegantClipboard
description: Quiet native clipboard workbench — dense, restrained, material-aware GPUI Kit UI
colors:
  primary: "#1a7770"
  primary-hover: "#17625c"
  primary-active: "#18524e"
  primary-text-dark: "#9bc8c3"
  primary-subtle-light: "#e3f0ee"
  primary-subtle-dark: "#1c3330"
  ring: "#6d9591"
  accent-blue: "#185abc"
  accent-orange: "#c2410c"
  accent-rouge: "#9d2933"
  surface-light: "#ffffff"
  surface-dark: "#22252b"
  canvas-light: "#ffffff"
  canvas-dark: "#15171b"
  foreground-light: "#18181b"
  foreground-dark: "#f1f3f5"
  subtle-light: "#fafafa"
  subtle-dark: "#1b1e23"
  muted-light: "#f4f4f5"
  muted-dark: "#2d3138"
  border-light: "#e4e4e7"
  border-dark: "#3a4049"
  emphasized-light: "#d4d4d8"
  emphasized-dark: "#515b69"
  secondary-text-light: "#52525b"
  secondary-text-dark: "#b5becb"
  material-fill-light: "#f2f3f5"
  material-fill-dark: "#414752"
  danger: "#dc2626"
  danger-dark: "#f87171"
  success: "#16a34a"
  success-dark: "#4ade80"
  warning: "#b45309"
  warning-dark: "#fbbf24"
  overlay-ink: "#09090b"
typography:
  title:
    fontFamily: "Microsoft YaHei UI, .SystemUIFont, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: 1.3
  dialog:
    fontFamily: "Microsoft YaHei UI, .SystemUIFont, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
  body:
    fontFamily: "Microsoft YaHei UI, .SystemUIFont, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.6
  label:
    fontFamily: "Microsoft YaHei UI, .SystemUIFont, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.4
rounded:
  none: "0px"
  sm: "4px"
  md: "8px"
  lg: "16px"
  system: "theme-default"
spacing:
  1: "4px"
  2: "8px"
  3: "12px"
  4: "16px"
  6: "24px"
  page: "12px"
  control: "32px"
  control-large: "36px"
  control-compact: "28px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "#ffffff"
    rounded: "{rounded.system}"
    height: "{spacing.control-large}"
    padding: "0 16px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
    textColor: "#ffffff"
  button-secondary:
    backgroundColor: "{colors.muted-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.system}"
    height: "{spacing.control}"
  button-danger:
    backgroundColor: "#b91c1c"
    textColor: "#ffffff"
    rounded: "{rounded.system}"
    height: "{spacing.control-large}"
  input-field:
    backgroundColor: "{colors.emphasized-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.system}"
    height: "{spacing.control-large}"
    padding: "0 12px"
  history-card:
    backgroundColor: "{colors.surface-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.system}"
    padding: "10px"
  search-input:
    backgroundColor: "{colors.emphasized-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.system}"
    height: "{spacing.control}"
    typography: "{typography.body}"
  tab-segment:
    backgroundColor: "{colors.muted-light}"
    textColor: "{colors.secondary-text-light}"
    rounded: "{rounded.system}"
    height: "{spacing.control}"
  tab-segment-active:
    backgroundColor: "{colors.surface-light}"
    textColor: "{colors.primary}"
  dialog-shell:
    backgroundColor: "{colors.surface-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.system}"
    width: "448px"
    padding: "16px"
---

# Design System: ElegantClipboard

## Overview

**Creative North Star: "The Quiet Instrument"**

ElegantClipboard’s UI is a precision desktop instrument: summoned, scanned, dismissed. Personality is restrained, slightly soft, and native-feeling — never loud. Density is high but orderly: history cards, metadata, and controls pack information without marketing chrome or decorative glass stacks.

The visual system is Chakra-inspired semantics projected onto GPUI Kit: one accent voice, cool zinc neutrals, light/dark parity, and optional OS materials (frosted / mica / opaque). Motion is short and state-bearing (150 / 200 / 300 ms Chakra curves). Global `theme.shadow` and `theme.focus_ring` stay off; depth and focus come from tonal fills, hairlines, and Kit’s thin control outlines.

Confirmed rejections: marketing hero/illustration bands; neon glassmorphism or heavy shadow piles; thick custom focus rings; browser-chrome metaphors (address bar / multi-tab app shell).

**Key Characteristics:**
- Operate-mode density over expression
- GPUI Kit catalog controls only for catalog interactions
- Material shell vs solid popover split
- One accent family at a time (default 鸭青); neutrals stay zinc
- Motion respects OS reduce-motion; feedback stays legible without spectacle

## Colors

Cool zinc neutrals carry structure; a single selectable accent carries all “this is interactive / selected” meaning. Status colors are reserved for danger, success, and warning only.

### Primary
- **鸭青 Cyan** (`#1a7770`): Documented default accent fill for primary buttons, progress, drag border, and strong actions. Hover `#17625c`, active `#18524e`. White labels on the fill.
- **Primary on dark text** (`#9bc8c3`): Accent-colored labels and selected tab text in dark mode (fill itself is too light for body text on frosted washes).
- **Ring / selection stroke** (`#6d9591`): List active border, selection wash driver, focus-adjacent accent outline. Selection fill uses ring at ~20% light / ~35% dark opacity.
- **Subtle wash**: light `#e3f0ee`, dark `#1c3330` — selected rows, sidebar accent, drop targets via material-aware tinting.

**Alternate accents** (user setting; same roles, swap the palette): Google Blue `#185abc`, Vibrant Orange `#c2410c`, Rouge 胭脂 `#9d2933`. Runtime code may still cold-start on Google Blue until preference says otherwise; design authority for new work is 鸭青 unless the active preference is another accent.

### Neutral
- **Surface** (light `#ffffff` / dark `#22252b`): Solid panels — popover, menus, dialogs, opaque cards. Popovers stay opaque even when the shell is material.
- **Canvas** (light `#ffffff` / dark `#15171b`): Window shell base before material compositing.
- **Foreground** (light `#18181b` / dark `#f1f3f5`): Primary ink.
- **Secondary text** (light `#52525b` / dark `#b5becb`): Metadata, sidebar labels, tab idle, scrollbar thumb base.
- **Subtle / muted / emphasized / border**: zinc ladder for tracks, secondary buttons, inputs, and hairlines (`#fafafa`→`#e4e4e7` light; `#1b1e23`→`#3a4049` dark).
- **Material control fill** (light `#f2f3f5` / dark `#414752`): Solid token under segmented selected faces when Frosted/Mica so Kit shadows do not bleed; list/card content uses low ink coverage (~13%) on the system backdrop instead of painting a second full shell.

### Semantic
- **Danger**: light `#dc2626` / dark `#f87171`; danger buttons use deeper reds (`#b91c1c`…).
- **Success**: light `#16a34a` / dark `#4ade80`.
- **Warning**: light `#b45309` / dark `#fbbf24`.
- **Overlay scrim**: `#09090b` at 45% light / 70% dark.

### Named Rules
**The One Accent Rule.** Only the active accent palette may color interactive emphasis (primary, selection, links, rings). Neutrals never compete with a second brand hue on the same screen.

**The Popover Is Solid Rule.** Dialogs, menus, notifications, floating action bars, and hover reading chrome use opaque `popover`/`surface`. Translucency belongs to the window shell and shared list wash — not to every floating layer.

**The Hairline Rule.** Under materials, shared separators use ~55% border alpha so they read as tonal cuts across the backdrop; controls that need a hard edge use `emphasized` / `input`, not a second border color family.

## Typography

**Display Font:** none (no marketing display face)
**Body Font:** Microsoft YaHei UI on Windows; `.SystemUIFont` on macOS/Linux; user-overridable UI / card / preview families
**Label/Mono Font:** same UI family (no separate mono token)

**Character:** System UI sans for bilingual CN/EN tooling. Readable at 12–18px in dense lists. No display serif, no uppercase tracking branding.

### Hierarchy
- **Title** (semibolded ~600, 18px / ~1.3): Onboarding step titles (22px / 30px line in that surface), dialog titles, settings section emphasis.
- **Dialog body** (400, 16px): Confirm copy and short forms.
- **Body** (400, 14px, line-height 1.6): Default UI, cards, settings labels; preview line height = 14 × 1.6; hover body uses 22.4px line box.
- **Label / small** (400, 12px): Settings descriptions, metadata, captions.
- **Onboarding title exception:** 22px semibold / 30px line — local to first-run steps, not a global display scale.

Default UI font size is 14px. The card family can follow the interface family, and the hover preview family can follow the card family. The three font sizes are independent.

### Named Rules
**The No Display Face Rule.** Do not introduce a decorative display font for chrome. Hierarchy is size and weight only inside the system UI family.

**The Metadata Whisper Rule.** Time, size, and source stay on secondary text at small size; they never steal accent color except when the row is selected.

## Layout

Logical-pixel spacing scale: 4 / 8 / 12 / 16 / 24. Page padding is 12px; search, filters, list, and status bar share that inset.

- **Main clipboard window:** borderless, no title bar; 30px drag grip above search; 28px bottom status bar; minimum client ~320×480 so search + first card + status remain visible.
- **History list:** full filtered set in memory; `VirtualList` renders visible rows only. Card density Compact/Standard/Spacious → padding 8 / 10 / 12. Standard two-line text ~96px row; image ~124px; +20px per extra preview line.
- **Category TabBar:** Filling Space, default 32px, equal-width visible tabs; groups via popover on the same row (group bar 42px).
- **Settings:** Kit Settings shell; sidebar 184–224px (initial 200); field columns prefer ~360px and reflow; min window ~700×420; 14px labels, 12px descriptions, 16px major gaps, 8px field gaps.
- **Dialogs:** default width 448px; compact 400px; centered with Kit overlay.
- **Hover preview:** beside main window when possible; text baseline ~460×300; image fit 480×480 (large mode up to 70% work area).
- **Drag edge auto-scroll:** 32px zones, 60ms tick.

No web breakpoint grid — this is a resizable desktop utility, not a responsive marketing site.

### Named Rules
**The Twelve-Pixel Gutter Rule.** Content left/right inset stays 12px unless a surface documents an exception (e.g. dialog internal padding).

**The Virtual List Rule.** Long history never switches to paginated “page 1 of N” chrome; height virtualization is the density strategy.

## Elevation & Depth

Default resting UI is flat and tonal. Depth comes from (1) light/dark zinc steps, (2) OS material on the shell, (3) sparse shadows on floating tool chrome only. Global theme shadow flag is **off**.

### Shadow Vocabulary
- **Popover / float** (`0 2px 12px rgba(0,0,0,0.20)`): menus, floating action bars, return-to-top, hover chrome that must separate from the list.
- **Control lift** (`0 1px 3px rgba(0,0,0,0.12)`): small elevated controls that need a hint of separation.
- **Material mode:** shell and shared containers stay clear/translucent; do not multiply shell tints on every nested panel. Segmented selected faces may use a solid `tokens.background` fill so Kit’s built-in selected shadow cannot grey-out through glass.

### Named Rules
**The Flat-By-Default Rule.** Cards and list rows do not carry drop shadows at rest. Selection is accent wash + ring stroke, not elevation.

**The Shell vs Chrome Rule.** Translucency is a window-material choice. Floating decision UI (dialog/menu/notification) stays solid for legibility and shadow honesty.

## Shapes

Corner radius is user-preferenced: System (Kit theme defaults), off (0), 4px, or 16px — applied to `theme.radius` / `radius_lg`. **Never** change OS native window corner radius; only in-app control/card radii.

- Controls and cards share the active theme radius.
- Scrollbar thumb: capsule, 4px radius, 3→6→8px width (idle/hover/active) in a 12px rail, 2px inset, min length 24px.
- Images in hover preview: inner corner follows `radius_lg`; outer frame is transparent (shadow only).
- Borders: 1px hairlines; no thick framed “card decks”.

### Named Rules
**The Radius Is a Preference Rule.** Do not hard-code a one-off radius fashion per feature; read the theme radius tokens.

**The Window Chrome Is Native Rule.** App-drawn radii stop at controls and content; system window shape stays system.

## Components

Philosophy: **refined and restrained** — official GPUI Kit components for catalog interactions; GPUI primitives only for app-specific content (history card hit targets, drag grips, hover canvas).

### Buttons
- **Shape:** theme radius; primary/secondary/danger from Kit color tokens.
- **Primary:** accent fill, white label, height 36px when paired with main actions; padding horizontal ~16px.
- **Secondary / neutral:** muted/control fills, foreground ink, often 32px.
- **Danger:** deep red fill for destructive confirms.
- **Compact:** 28px for about-row and dense tool rows.
- **Hover/active:** tokenized `*_hover` / `*_active` steps; color transitions 150ms in premultiplied RGBA (no HSL hue spin). Material cards may snap color to avoid composite fringes; action-bar opacity still eases 150ms.
- **Icon buttons:** e.g. return-to-top 36px outline + light control shadow.

### Cards / History rows
- Variable-height VirtualList rows; text starts top-left with density padding; metadata row 20px along the bottom (time, size, source).
- Media/file: 48×36 thumb left, title + meta right.
- Hover reveals 28px floating action strip (select, favorite, pin, copy, delete) — popover surface, hairline, light shadow; hidden while dragging; selection does not force the strip open.
- Selected/keyboard target: accent wash + ring border (same target model for pointer and keys).
- Success flourishes: brief primary-tint mask + check (copy) or heart (favorite) ~500ms; none under reduce-motion; paste/merge/failure do not play the flourish.

### Inputs / Fields
- Kit Input/NumberInput/Select/Textarea; large fields 36px; search Medium 32px / 14px.
- Material: input fill from emphasized or dark translucent foreground recipe — not a custom third surface.
- Focus: Kit thin border/1px line; **no** global thick focus ring (`focus_ring = false`).
- Shortcuts shown with Kit `Kbd`; inside TabBar labels, Kbd chrome is cleared so the tab state owns the look.

### Navigation / chrome
- Main window: search → category TabBar + group popover → list → status bar.
- Settings: Kit `Settings` / `SettingPage` / `SettingGroup` / `SettingItem` + full-width GroupBox; Accordion for long lists.
- Tray + global hotkey are OS integration, not in-window nav metaphors.

### Dialogs & feedback
- Confirmations: shared dialog shell/title/footer/copy helpers; Kit Dialog + overlay.
- Transient success/error: Kit `Notification` via Root when appropriate; main window also mirrors into StatusBar (~3s success / ~6s error). No success toast spam for silent background refresh.
- Inline `Alert` only for persistent actionable conditions (e.g. material capability downgrade).

### Tabs / chips / segmented
- Category and settings mutually-exclusive choices: Kit TabBar Filling Space (equal width).
- Segmented track uses muted control; active segment solid/selected control with primary-tinted label.

### Signature: Hover preview
- Transparent, mouse-passthrough, non-activating layer; no title bar or path footer.
- HTML via markdown TextView path; images scale with Ctrl+wheel; external info tag for type/size/zoom hints.
- Opens after ~300ms hover; closes 600ms after leaving card and preview.

### Signature: Onboarding
- Three steps inside the main panel (not a separate wizard window): shortcut, practice on isolated demo data, preferences.
- Title 22/30; body scrolls; footer prev/next fixed; no step dots; no Esc dismiss; no skip.

## Do's and Don'ts

### Do:
- **Do** use GPUI Kit catalog components for buttons, inputs, dialogs, tabs, switches, checkboxes, settings chrome, notifications, virtual lists, and Kbd.
- **Do** read colors from theme semantic tokens (`primary`, `muted_foreground`, `popover`, …) after `design_system::apply` — never local light/dark hex tables in features.
- **Do** keep main-window chrome borderless with the grip + status bar pattern; settings may use Kit TitleBar.
- **Do** honor reduce-motion: cut spatial travel, keep opacity/color state changes that encode feedback.
- **Do** keep popovers/menus/dialogs on solid surfaces; put materials on the shell.
- **Do** match pointer hover and keyboard focus to the same selected row model.
- **Do** stay within the spacing scale (4–24) and control heights (28 / 32 / 36).

### Don't:
- **Don't** add a second component library or hand-roll a Kit equivalent control.
- **Don't** enable thick global focus rings or decorative multi-stop neon gradients.
- **Don't** put marketing heroes, illustrative empty-state mascots, or web app tab-strip chrome into the utility shell.
- **Don't** silently degrade rich clipboard formats in UI copy affordances; errors for missing formats stay explicit (product constraint reflected in quiet, honest UI).
- **Don't** paint nested panels with independent “glass” stacks that fight DWM/AppKit blur.
- **Don't** invent a display typeface or a second accent on the same view.
- **Don't** use shadows on resting list cards to fake hierarchy — use density, type, and selection wash.
- **Don't** claim iOS HIG / Material 3 mobile patterns; this system is desktop GPUI despite `adaptive` product platform tagging.
