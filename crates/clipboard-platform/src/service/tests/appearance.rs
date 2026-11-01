use super::*;
use clipboard_core::preferences::CornerRadiusPreference;

#[test]
fn corner_radius_acknowledgement_preserves_value_after_rejected_change() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for preference in [
        CornerRadiusPreference::Fixed(12),
        CornerRadiusPreference::System,
    ] {
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::SetCornerRadius(preference))?;
        let saved = wait_for(
            &events,
            Duration::from_secs(5),
            "radius save timed out",
            |event| match event {
                Event::CornerRadiusSaved(result) => Some(result),
                _ => None,
            },
        );
        assert_eq!(saved.unwrap(), preference);
        for radius in [8, 17] {
            service.send(Command::SetCornerRadius(CornerRadiusPreference::Fixed(
                radius,
            )))?;
            let rejected = wait_for(
                &events,
                Duration::from_secs(5),
                "radius rejection timed out",
                |event| match event {
                    Event::CornerRadiusSaved(result) => Some(result),
                    _ => None,
                },
            );
            assert!(rejected.is_err());
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_preferences.corner_radius, preference);
    }
    Ok(())
}
