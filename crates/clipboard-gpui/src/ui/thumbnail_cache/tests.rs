use super::*;
use image::{ImageBuffer, Rgba};

fn key(path: PathBuf) -> ThumbnailKey {
    ThumbnailKey::new(path, "source-hash".into(), false, (12., 8.), 2.)
}

fn ready(result: ThumbnailResult) -> Arc<RenderImage> {
    match result.image.unwrap() {
        Thumbnail::Ready(image) => image,
        _ => panic!("expected decoded thumbnail"),
    }
}

#[test]
fn scales_real_pixels_to_dpi_bounds_preserving_aspect_color_and_alpha() {
    let dir = tempfile::tempdir().unwrap();
    for (width, height, expected) in [
        (200, 100, (24, 12)),
        (1440, 2560, (9, 16)),
        (3072, 1728, (24, 14)),
    ] {
        let path = dir.path().join(format!("{width}x{height}.png"));
        // Different top/bottom colors detect cropping as well as aspect-ratio changes.
        ImageBuffer::from_fn(width, height, |_, y| {
            if y < height / 2 {
                Rgba([211u8, 57, 19, 127])
            } else {
                Rgba([13u8, 89, 233, 255])
            }
        })
        .save(&path)
        .unwrap();
        let image = decode_thumbnail(&key(path)).unwrap().unwrap();
        assert_eq!((image.size(0).width.0, image.size(0).height.0), expected);
        let pixels = image.as_bytes(0).unwrap();
        assert_eq!(&pixels[..4], &[19, 57, 211, 127]);
        assert_eq!(&pixels[pixels.len() - 4..], &[233, 89, 13, 255]);
    }
    // Small images are not needlessly upscaled, and their original bytes remain intact.
    let path = dir.path().join("small.png");
    ImageBuffer::from_pixel(2, 1, Rgba([1u8, 2, 3, 255]))
        .save(&path)
        .unwrap();
    let image = decode_thumbnail(&key(path)).unwrap().unwrap();
    assert_eq!((image.size(0).width.0, image.size(0).height.0), (2, 1));
}

#[test]
fn evicted_jobs_do_not_refill_cache_or_overwrite_reentered_requests() {
    let mut cache = ThumbnailCache::default();
    let first = key("first.png".into());
    let second = key("second.png".into());
    cache.retain(vec![first.clone(), second.clone()]);
    let old = cache.next_job().unwrap();
    let other = cache.next_job().unwrap();
    assert!(cache.next_job().is_none());
    cache.retain(vec![]);
    cache.retain(vec![first.clone()]);
    // Eviction does not release a worker slot before its real work has finished.
    assert!(cache.next_job().is_none());
    assert!(
        cache
            .finish(
                other.token,
                ThumbnailResult {
                    version: None,
                    image: Some(Thumbnail::Original)
                }
            )
            .is_none()
    );
    let current = cache.next_job().unwrap();
    assert!(
        cache
            .finish(
                old.token,
                ThumbnailResult {
                    version: None,
                    image: Some(Thumbnail::Original)
                }
            )
            .is_none()
    );
    assert!(cache.source(&first).is_none());
    assert_eq!(
        cache.finish(
            current.token,
            ThumbnailResult {
                version: None,
                image: Some(Thumbnail::Original)
            }
        ),
        Some(first.path.clone())
    );
    assert!(matches!(
        cache.source(&first),
        Some(ImageSource::Resource(_))
    ));
}

#[test]
fn schedules_first_occurrences_in_latest_visible_order_without_duplicate_jobs() {
    let mut cache = ThumbnailCache::default();
    let first = key("first.png".into());
    let second = key("second.png".into());
    let third = key("third.png".into());
    let fourth = key("fourth.png".into());
    cache.retain([
        first.clone(),
        third.clone(),
        third.clone(),
        second.clone(),
        fourth.clone(),
    ]);
    let initial = cache.next_job().unwrap();
    assert_eq!(initial.key, first);
    // Scrolling changes pending priorities, but neither duplicates nor an active key
    // may take another worker slot.
    cache.retain([
        fourth.clone(),
        third.clone(),
        fourth.clone(),
        second.clone(),
        first.clone(),
    ]);
    let next = cache.next_job().unwrap();
    assert_eq!(next.key, fourth);
    assert!(cache.next_job().is_none());
    cache.finish(
        initial.token,
        ThumbnailResult {
            version: None,
            image: Some(Thumbnail::Original),
        },
    );
    let third_job = cache.next_job().unwrap();
    assert_eq!(third_job.key, third);
    for job in [next, third_job] {
        cache.finish(
            job.token,
            ThumbnailResult {
                version: None,
                image: Some(Thumbnail::Original),
            },
        );
    }
    let last = cache.next_job().unwrap();
    assert_eq!(last.key, second);
    assert!(cache.next_job().is_none());
    cache.finish(
        last.token,
        ThumbnailResult {
            version: None,
            image: Some(Thumbnail::Original),
        },
    );
    cache.retain([second, fourth, first, third]);
    assert!(cache.next_job().is_none());
}

#[test]
fn new_thumbnails_precede_due_external_rechecks_in_visible_order() {
    let mut cache = ThumbnailCache::default();
    let mut first = key("first.png".into());
    first.external = true;
    let mut second = key("second.png".into());
    second.external = true;
    cache.retain([first.clone(), second.clone()]);
    for _ in 0..2 {
        let job = cache.next_job().unwrap();
        cache.finish(
            job.token,
            ThumbnailResult {
                version: None,
                image: Some(Thumbnail::Original),
            },
        );
    }
    assert!(cache.next_job().is_none());
    for entry in cache.entries.values_mut() {
        entry.checked = Instant::now() - FILE_RECHECK;
    }
    let new = key("new.png".into());
    cache.retain([second.clone(), first.clone(), second.clone(), new.clone()]);
    let initial = cache.next_job().unwrap();
    assert_eq!(initial.key, new);
    let recheck = cache.next_job().unwrap();
    assert_eq!(recheck.key, second);
    assert!(cache.next_job().is_none());
    cache.finish(
        recheck.token,
        ThumbnailResult {
            version: None,
            image: None,
        },
    );
    let remaining = cache.next_job().unwrap();
    assert_eq!(remaining.key, first);
    for job in [initial, remaining] {
        cache.finish(
            job.token,
            ThumbnailResult {
                version: None,
                image: Some(Thumbnail::Original),
            },
        );
    }
    assert!(cache.next_job().is_none());
}

#[test]
fn external_version_reuses_unchanged_pixels_and_reloads_changed_or_failed_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.png");
    ImageBuffer::from_pixel(8, 4, Rgba([255u8, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let mut source = key(path.clone());
    source.external = true;
    let first = ThumbnailJob {
        token: 1,
        key: source.clone(),
        previous: None,
    }
    .run();
    let version = first.version.clone();
    assert_eq!(&ready(first).as_bytes(0).unwrap()[..4], &[0, 0, 255, 255]);
    assert!(
        ThumbnailJob {
            token: 2,
            key: source.clone(),
            previous: version.clone()
        }
        .run()
        .image
        .is_none()
    );
    // A different file length guarantees detection even on a coarse timestamp filesystem.
    ImageBuffer::from_pixel(81, 41, Rgba([0u8, 255, 0, 255]))
        .save(&path)
        .unwrap();
    let changed = ThumbnailJob {
        token: 3,
        key: source.clone(),
        previous: version,
    }
    .run();
    assert_eq!(&ready(changed).as_bytes(0).unwrap()[..4], &[0, 255, 0, 255]);
    std::fs::write(&path, b"broken image").unwrap();
    let failed = ThumbnailJob {
        token: 4,
        key: source.clone(),
        previous: None,
    }
    .run();
    assert!(matches!(failed.image, Some(Thumbnail::Failed(_))));
    assert!(failed.version.is_none());
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        ThumbnailJob {
            token: 5,
            key: source,
            previous: None
        }
        .run()
        .image,
        Some(Thumbnail::Failed(_))
    ));
}

#[test]
fn animated_formats_fall_back_by_content_even_with_wrong_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("animation.png");
    // Format identification deliberately precedes decode so GPUI retains animation ownership.
    for signature in [b"GIF89a".as_slice(), b"RIFF\x00\x00\x00\x00WEBP".as_slice()] {
        std::fs::write(&path, signature).unwrap();
        assert!(decode_thumbnail(&key(path.clone())).unwrap().is_none());
    }
}

#[test]
fn drag_arc_keeps_pixels_until_snapshot_releases_them() {
    let mut cache = ThumbnailCache::default();
    let source = key("drag.png".into());
    cache.retain(vec![source.clone()]);
    let job = cache.next_job().unwrap();
    let image = Arc::new(RenderImage::new(vec![Frame::new(ImageBuffer::from_pixel(
        1,
        1,
        Rgba([1u8, 2, 3, 255]),
    ))]));
    cache.finish(
        job.token,
        ThumbnailResult {
            version: None,
            image: Some(Thumbnail::Ready(image.clone())),
        },
    );
    let snapshot = cache.source(&source).unwrap();
    drop(image);
    cache.retain(vec![]);
    assert!(cache.released_images().is_empty());
    let ImageSource::Render(drag_image) = &snapshot else {
        panic!("render image")
    };
    assert_eq!(drag_image.as_bytes(0).unwrap(), &[1, 2, 3, 255]);
    drop(snapshot);
    assert_eq!(cache.released_images().len(), 1);
    assert!(cache.released_images().is_empty());
}
