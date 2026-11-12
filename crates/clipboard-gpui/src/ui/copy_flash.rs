use super::*;
use std::time::Duration;

pub(super) const FLASH_DURATION: Duration = Duration::from_millis(500);
const ICON_DURATION_MS: u128 = 500;
const FLASH_DURATION_MS: u128 = 500;
const ICON_SIZE: f32 = 32.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardFlashKind {
    Copy,
    Favorite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CopyFlash {
    pub id: i64,
    pub token: u64,
    pub kind: CardFlashKind,
}

const THIN_CIRCLE_CHECK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m16 9-5.5 5.5L8 12"/></svg>"#;
const THIN_HEART: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"><path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/></svg>"#;

pub(super) fn shows_favorite_flash(currently_favorite: bool) -> bool {
    !currently_favorite
}

pub(super) fn begin_card_flash(
    flash: &mut Option<CopyFlash>,
    token_source: &mut u64,
    id: i64,
    kind: CardFlashKind,
) -> u64 {
    *token_source = token_source.wrapping_add(1);
    let token = *token_source;
    *flash = Some(CopyFlash { id, token, kind });
    token
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CopyFlashFrame {
    pub overlay_opacity: f32,
    pub icon_scale: f32,
    pub icon_opacity: f32,
}

pub(super) fn record_copy_success(
    flash: &mut Option<CopyFlash>,
    token_source: &mut u64,
    purpose: CopyPurpose,
    id: i64,
) -> Option<u64> {
    if !matches!(purpose, CopyPurpose::Copy) {
        return None;
    }
    Some(begin_card_flash(
        flash,
        token_source,
        id,
        CardFlashKind::Copy,
    ))
}

pub(super) fn clear_copy_flash(flash: &mut Option<CopyFlash>, token: u64) -> bool {
    if flash.is_some_and(|current| current.token == token) {
        *flash = None;
        true
    } else {
        false
    }
}

pub(super) fn copy_flash_frame(elapsed: Duration) -> Option<CopyFlashFrame> {
    let elapsed_ms = elapsed.as_millis();
    if elapsed_ms > FLASH_DURATION_MS {
        return None;
    }
    let flash_t = elapsed_ms as f32 / FLASH_DURATION_MS as f32;
    let icon_t = (elapsed_ms as f32 / ICON_DURATION_MS as f32).min(1.);
    Some(CopyFlashFrame {
        overlay_opacity: overlay_opacity(flash_t),
        icon_scale: icon_scale(icon_t),
        icon_opacity: icon_opacity(icon_t),
    })
}

pub(super) fn overlay(token: u64, kind: CardFlashKind, color: Hsla) -> impl IntoElement {
    let icon_color = color;
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(color.opacity(0.1))
        .with_animation(
            ("history-copy-flash", token as usize),
            Animation::new(FLASH_DURATION),
            move |overlay, progress| {
                let elapsed =
                    Duration::from_millis((progress * FLASH_DURATION_MS as f32).round() as u64);
                let frame = copy_flash_frame(elapsed).unwrap_or(CopyFlashFrame {
                    overlay_opacity: 0.,
                    icon_scale: 1.,
                    icon_opacity: 1.,
                });
                overlay.opacity(frame.overlay_opacity).child(
                    div()
                        .size(px(ICON_SIZE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(frame.icon_opacity)
                        .child(flash_icon(
                            kind,
                            (ICON_SIZE * frame.icon_scale).max(0.),
                            icon_color,
                        )),
                )
            },
        )
}

fn flash_icon(kind: CardFlashKind, size: f32, color: Hsla) -> Icon {
    let data: &[u8] = match kind {
        CardFlashKind::Copy => THIN_CIRCLE_CHECK,
        CardFlashKind::Favorite => THIN_HEART,
    };
    Icon::empty().data(data).size(px(size)).text_color(color)
}

fn overlay_opacity(progress: f32) -> f32 {
    if progress <= 0.5 {
        1.
    } else {
        1. - ease_out(unit_between(progress, 0.5))
    }
}

fn icon_scale(progress: f32) -> f32 {
    keyed(progress, 0.3, 1.15, 1.)
}

fn icon_opacity(progress: f32) -> f32 {
    if progress >= 0.5 {
        1.
    } else {
        spring(unit_between(progress, 0.))
    }
}

fn keyed(progress: f32, start: f32, mid: f32, end: f32) -> f32 {
    if progress <= 0.5 {
        lerp(start, mid, spring(unit_between(progress, 0.)))
    } else {
        lerp(mid, end, spring(unit_between(progress, 0.5)))
    }
}

fn unit_between(progress: f32, start: f32) -> f32 {
    ((progress - start) / 0.5).clamp(0., 1.)
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn ease_out(progress: f32) -> f32 {
    bezier_y(0., 0., 0.58, 1., progress)
}

fn spring(progress: f32) -> f32 {
    bezier_y(0.34, 1.56, 0.64, 1., progress)
}

fn bezier_y(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0. {
        return 0.;
    }
    if x >= 1. {
        return 1.;
    }
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..24 {
        let t = (low + high) * 0.5;
        if sample_bezier(t, x1, x2) < x {
            low = t;
        } else {
            high = t;
        }
    }
    sample_bezier((low + high) * 0.5, y1, y2)
}

fn sample_bezier(t: f32, first: f32, second: f32) -> f32 {
    let remaining = 1. - t;
    3. * remaining * remaining * t * first + 3. * remaining * t * t * second + t * t * t
}

#[cfg(test)]
mod tests {
    use super::{
        CardFlashKind, CopyFlash, CopyPurpose, begin_card_flash, clear_copy_flash,
        copy_flash_frame, record_copy_success, shows_favorite_flash,
    };
    use std::time::Duration;

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
    }

    #[test]
    fn copy_flash_keyframes_follow_the_main_branch_timing() {
        let start = copy_flash_frame(Duration::ZERO).unwrap();
        near(start.overlay_opacity, 1.);
        near(start.icon_scale, 0.3);
        near(start.icon_opacity, 0.);

        let icon_midpoint = copy_flash_frame(Duration::from_millis(250)).unwrap();
        near(icon_midpoint.overlay_opacity, 1.);
        near(icon_midpoint.icon_scale, 1.15);
        near(icon_midpoint.icon_opacity, 1.);

        let later = copy_flash_frame(Duration::from_millis(300)).unwrap();
        assert!(later.overlay_opacity < 1.);
        assert!(later.overlay_opacity > 0.);
        near(later.icon_opacity, 1.);
        assert!(later.icon_scale > 1.);
        assert!(later.icon_scale < 1.15);

        let end = copy_flash_frame(Duration::from_millis(500)).unwrap();
        near(end.overlay_opacity, 0.);
        near(end.icon_scale, 1.);
        near(end.icon_opacity, 1.);
        assert!(copy_flash_frame(Duration::from_millis(501)).is_none());
    }

    #[test]
    fn only_a_copy_success_replaces_the_visible_flash() {
        let mut flash = None;
        let mut token_source = 0;
        let first =
            record_copy_success(&mut flash, &mut token_source, CopyPurpose::Copy, 7).unwrap();
        assert_eq!(
            flash,
            Some(CopyFlash {
                id: 7,
                token: first,
                kind: CardFlashKind::Copy,
            })
        );
        assert!(
            record_copy_success(
                &mut flash,
                &mut token_source,
                CopyPurpose::Paste { request_id: 3 },
                9,
            )
            .is_none()
        );
        assert_eq!(flash.unwrap().id, 7);
        assert!(!clear_copy_flash(&mut flash, first.wrapping_add(1)));

        let second =
            record_copy_success(&mut flash, &mut token_source, CopyPurpose::Copy, 8).unwrap();
        assert_ne!(first, second);
        assert_eq!(flash.unwrap().id, 8);
        assert!(!clear_copy_flash(&mut flash, first));
        assert!(clear_copy_flash(&mut flash, second));
        assert!(flash.is_none());
    }

    #[test]
    fn favorite_flash_replaces_copy_and_skips_unfavorite() {
        assert!(!shows_favorite_flash(true));
        assert!(shows_favorite_flash(false));
        let mut flash = None;
        let mut token_source = 0;
        let copied = begin_card_flash(&mut flash, &mut token_source, 4, CardFlashKind::Copy);
        let favorited = begin_card_flash(&mut flash, &mut token_source, 4, CardFlashKind::Favorite);
        assert_ne!(copied, favorited);
        assert_eq!(flash.unwrap().kind, CardFlashKind::Favorite);
        assert!(!clear_copy_flash(&mut flash, copied));
        assert!(clear_copy_flash(&mut flash, favorited));
    }
}
