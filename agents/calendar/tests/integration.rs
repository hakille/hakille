use agent_calendar::{CalendarEvent, CalendarManager};

fn evt(uid: &str, start: &str, end: &str) -> CalendarEvent {
    CalendarEvent {
        uid: uid.to_string(),
        summary: format!("Evt {uid}"),
        description: String::new(),
        start_time: start.to_string(),
        end_time: end.to_string(),
        location: None,
        recurrence: None,
        status: "CONFIRMED".to_string(),
    }
}

/// Intégration calendar : seed + détection de conflit + ajout disjoint.
#[test]
fn test_calendar_integration_seed_and_conflict() {
    let mut mgr = CalendarManager::new();
    assert_eq!(mgr.events().len(), 2, "seed de 2 événements attendu");

    // Chevauchement du 1er seed (10:00–11:30) → conflit + refus d'ajout.
    let conflict = evt(
        "evt-test-conflict",
        "2026-10-09T10:30:00Z",
        "2026-10-09T11:00:00Z",
    );
    let conflicts = mgr.find_conflicts(&conflict);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].uid, "evt-20261009-01");
    assert!(mgr.add_event(conflict).is_err());
    assert_eq!(mgr.events().len(), 2);

    // Créneau disjoint → accepté.
    let free = evt(
        "evt-test-free",
        "2026-10-09T15:00:00Z",
        "2026-10-09T16:00:00Z",
    );
    assert!(mgr.find_conflicts(&free).is_empty());
    assert!(mgr.add_event(free).is_ok());
    assert_eq!(mgr.events().len(), 3);

    // Filtrage par date + roundtrip ICS.
    assert_eq!(mgr.events_for_date("2026-10-09").len(), 3);
    let ics = CalendarManager::export_ics(mgr.events());
    let reparsed = CalendarManager::parse_ics(&ics).unwrap();
    assert_eq!(reparsed.len(), 3);
}
