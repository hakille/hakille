//! Fonctionnalités iCalendar réelles, sans moteur de fuseaux ni récurrence simulés.
use agent_calendar::CalendarManager;

fn calendar(properties: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:test-1\r\n{properties}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

#[test]
fn default_calendar_has_no_fabricated_events() {
    assert!(CalendarManager::new().events().is_empty());
    assert!(CalendarManager::default().events().is_empty());
    assert_eq!(CalendarManager::demo().events().len(), 2);
}

#[test]
fn all_day_defaults_to_next_day_and_end_is_exclusive() {
    let content = calendar("SUMMARY:Journée entière\r\nDTSTART;VALUE=DATE:20260228");
    let events = CalendarManager::parse_ics(&content).unwrap();
    assert_eq!(events[0].start_time, "20260228");
    assert_eq!(events[0].end_time, "20260301");
    let exported = CalendarManager::export_ics(&events).unwrap();
    let stamp = exported
        .lines()
        .find_map(|line| line.strip_prefix("DTSTAMP:"))
        .expect("estampille UTC de l'export");
    assert!(chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%SZ").is_ok());
    assert!(exported.contains("DTSTART;VALUE=DATE:20260228\r\n"));
    assert!(exported.contains("DTEND;VALUE=DATE:20260301\r\n"));
    assert_eq!(CalendarManager::parse_ics(&exported).unwrap(), events);
    let mut manager = CalendarManager::new();
    manager.import_ics(&content).unwrap();
    assert_eq!(manager.events_for_date("2026-02-28").len(), 1);
    assert!(manager.events_for_date("2026-03-01").is_empty());
    assert!(manager.events_for_date("").is_empty());
    assert!(manager.events_for_date("2026-02").is_empty());
}

#[test]
fn duration_and_multiday_queries_preserve_interval_semantics() {
    let content = calendar("DTSTART;VALUE=DATE:20260228\r\nDURATION:P3D");
    let mut manager = CalendarManager::new();
    manager.import_ics(&content).unwrap();
    assert_eq!(manager.events()[0].end_time, "20260303");
    assert_eq!(manager.events_for_date("2026-03-02").len(), 1);
    assert!(manager.events_for_date("2026-03-03").is_empty());
    let events =
        CalendarManager::parse_ics(&calendar("DTSTART:20261231T233000Z\r\nDURATION:PT2H15M"))
            .unwrap();
    assert_eq!(events[0].end_time, "20270101T014500Z");
}

#[test]
fn absent_end_is_an_instant_not_a_blocking_event() {
    let mut manager = CalendarManager::new();
    let busy = CalendarManager::parse_ics(&calendar(
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z",
    ))
    .unwrap()
    .remove(0);
    manager.add_event(busy).unwrap();
    let mut instant = CalendarManager::parse_ics(&calendar("DTSTART:20261103T103000Z"))
        .unwrap()
        .remove(0);
    instant.uid = "instant".into();
    assert!(instant.end_time.is_empty());
    let exported = CalendarManager::export_ics(&[instant.clone()]).unwrap();
    assert!(!exported.contains("DTEND"));
    assert_eq!(CalendarManager::parse_ics(&exported).unwrap()[0], instant);
    assert!(manager.find_conflicts(&instant).unwrap().is_empty());
    manager.add_event(instant).unwrap();
    assert_eq!(manager.events_for_date("2026-11-03").len(), 2);
}

#[test]
fn timezone_and_rrule_are_preserved_but_never_silently_scheduled() {
    let content = calendar(
        "SUMMARY:Réunion\r\nDTSTART;TZID=Europe/Paris:20261025T023000\r\nDTEND;TZID=Europe/Paris:20261025T033000\r\nRRULE:FREQ=WEEKLY;COUNT=3",
    );
    let events = CalendarManager::parse_ics(&content).unwrap();
    assert_eq!(events[0].start_timezone.as_deref(), Some("Europe/Paris"));
    let exported = CalendarManager::export_ics(&events).unwrap();
    assert!(exported.contains("TZID=\"Europe/Paris\""));
    assert_eq!(CalendarManager::parse_ics(&exported).unwrap(), events);
    let mut manager = CalendarManager::new();
    assert!(
        manager
            .find_conflicts(&events[0])
            .unwrap_err()
            .contains("TZID")
    );
    assert!(manager.add_event(events[0].clone()).is_err());
    let recurring = CalendarManager::parse_ics(&calendar(
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z\r\nRRULE:FREQ=DAILY",
    ))
    .unwrap();
    assert!(
        manager
            .find_conflicts(&recurring[0])
            .unwrap_err()
            .contains("Récurrence")
    );
}

#[test]
fn text_escapes_quoted_parameter_colons_and_utf8_folding_roundtrip() {
    let content = calendar(
        "SUMMARY:Équipe\\, A\\;B\\\\C\r\nDESCRIPTION:ligne 1\\nligne 2\r\nLOCATION;ALTREP=\"https://example.test/salle\":Bureau\\, étage 2\r\nDTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z",
    );
    let mut events = CalendarManager::parse_ics(&content).unwrap();
    assert_eq!(events[0].summary, "Équipe, A;B\\C");
    assert_eq!(events[0].description, "ligne 1\nligne 2");
    assert_eq!(events[0].location.as_deref(), Some("Bureau, étage 2"));
    events[0].summary = "日本語🙂é".repeat(40);
    events[0].description = "texte\nBEGIN:VEVENT\nSUMMARY:injection".into();
    let output = CalendarManager::export_ics(&events).unwrap();
    assert!(output.split("\r\n").all(|line| line.len() <= 75));
    assert_eq!(output.matches("BEGIN:VEVENT\r\n").count(), 1);
    assert_eq!(CalendarManager::parse_ics(&output).unwrap(), events);
}

#[test]
fn import_is_atomic_and_duplicate_uids_are_refused() {
    let first = calendar("DTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z");
    let second = "BEGIN:VEVENT\r\nUID:test-2\r\nDTSTART:20261103T103000Z\r\nDTEND:20261103T113000Z\r\nEND:VEVENT\r\n";
    let content = first.replace("END:VCALENDAR", &format!("{second}END:VCALENDAR"));
    let mut manager = CalendarManager::new();
    assert!(manager.import_ics(&content).is_err());
    assert!(
        manager.events().is_empty(),
        "aucun préfixe de lot partiellement importé"
    );
    assert_eq!(manager.import_ics(&first).unwrap(), 1);
    assert!(manager.import_ics(&first).is_err());
    assert_eq!(manager.events().len(), 1);
}

#[test]
fn invalid_dates_types_durations_and_lossy_features_are_rejected() {
    for properties in [
        "DTSTART;VALUE=DATE:20260230",
        "DTSTART;VALUE=DATE:20261103\r\nDTEND:20261104T000000Z",
        "DTSTART;VALUE=DATE:20261103\r\nDURATION:PT24H",
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T090000Z",
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T100000Z",
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z\r\nDURATION:PT1H",
        "DTSTART:20261103T100000Z\r\nDURATION:P1M",
        "DTSTART:20261103T100000Z\r\nDURATION:P1W1D",
        "DTSTART:20261103T100000Z\r\nDURATION:PT1H1H",
        "DTSTART:20261103T100000Z\r\nDURATION:P1DT",
        "DTSTART;TZID=Europe/Paris:20261103T100000\r\nDURATION:P1D",
        "DTSTART;VALUE=DATE:99991231",
        "DTSTART:20261103T100000Z\r\nEXDATE:20261104T100000Z",
        "DTSTART:20261103T100000Z\r\nBEGIN:VALARM\r\nSUMMARY:Pas le titre\r\nEND:VALARM",
        "DTSTART:20261103T100000Z\r\nDTSTART:20261103T110000Z",
        "DTSTART:20261103T100000Z\r\nSTATUS:INVALID",
        "DTSTART:20261103T100000Z\r\nTRANSP:TRANSPARENT",
    ] {
        assert!(
            CalendarManager::parse_ics(&calendar(properties)).is_err(),
            "accepté : {properties}"
        );
    }
    assert!(
        CalendarManager::parse_ics(
            "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nEND:VTIMEZONE\r\nEND:VCALENDAR\r\n"
        )
        .is_err()
    );
    assert!(
        CalendarManager::parse_ics("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:incomplet").is_err()
    );
}

#[test]
fn invitations_and_cancellations_are_not_misread_as_published_events() {
    for method in ["CANCEL", "REQUEST", "REPLY"] {
        let content = calendar("DTSTART:20261103T100000Z")
            .replace("VERSION:2.0", &format!("VERSION:2.0\r\nMETHOD:{method}"));
        assert!(CalendarManager::parse_ics(&content).is_err());
    }
}

#[test]
fn subsecond_precision_is_refused_instead_of_truncated() {
    let content = calendar("DTSTART:2026-11-03T10:00:00.900Z\r\nDTEND:2026-11-03T11:00:00Z");
    assert!(CalendarManager::parse_ics(&content).is_err());
    let mut event = CalendarManager::parse_ics(&calendar(
        "DTSTART:20261103T100000Z\r\nDTEND:20261103T110000Z",
    ))
    .unwrap()
    .remove(0);
    event.start_time = "2026-11-03T10:00:00.900Z".into();
    assert!(CalendarManager::export_ics(&[event.clone()]).is_err());
    assert!(CalendarManager::new().find_conflicts(&event).is_err());
}

#[test]
fn utc_offset_export_is_canonical_and_cancelled_events_do_not_block() {
    let mut events = CalendarManager::parse_ics(&calendar(
        "DTSTART:2026-11-03T12:00:00+02:00\r\nDTEND:2026-11-03T13:00:00+02:00\r\nSTATUS:CANCELLED",
    ))
    .unwrap();
    assert_eq!(events[0].start_time, "20261103T100000Z");
    let mut manager = CalendarManager::new();
    manager.add_event(events[0].clone()).unwrap();
    events[0].uid = "active".into();
    events[0].status = "CONFIRMED".into();
    assert!(manager.find_conflicts(&events[0]).unwrap().is_empty());
    assert!(manager.add_event(events[0].clone()).is_ok());
}
