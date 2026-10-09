use annora_core::{CapabilityAction, CapabilityScope, NodeId, PredicateId, Timestamp, TripletId, TrustLevel};
use annora_memory::{ContextPartition, KnowledgeGraph, NodeValue, Triplet, TripletMetadata, TripletSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarEvent {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub start_time: String,
    pub end_time: String,
    pub location: Option<String>,
    pub recurrence: Option<String>,
    pub status: String,
}

pub struct CalendarManager {
    events: Vec<CalendarEvent>,
}

impl CalendarManager {
    pub fn new() -> Self {
        let mut mgr = Self { events: Vec::new() };
        mgr.seed_sample_events();
        mgr
    }

    fn seed_sample_events(&mut self) {
        self.events.push(CalendarEvent {
            uid: "evt-20261009-01".to_string(),
            summary: "Revue Architecture Cognitive Annora".to_string(),
            description: "Validation des 20 crates Rust et des modèles neuronaux.".to_string(),
            start_time: "2026-10-09T10:00:00Z".to_string(),
            end_time: "2026-10-09T11:30:00Z".to_string(),
            location: Some("Salle Turing & Visioconférence Sécurisée".to_string()),
            recurrence: None,
            status: "CONFIRMED".to_string(),
        });
        self.events.push(CalendarEvent {
            uid: "evt-20261009-02".to_string(),
            summary: "Déjeuner de Travail & Souveraineté Numérique".to_string(),
            description: "Point d'étape sur la conformité Biscuit v5 et l'audit Ed25519.".to_string(),
            start_time: "2026-10-09T12:30:00Z".to_string(),
            end_time: "2026-10-09T14:00:00Z".to_string(),
            location: Some("Restaurant Le Silicium".to_string()),
            recurrence: None,
            status: "CONFIRMED".to_string(),
        });
    }

    pub fn events(&self) -> &[CalendarEvent] {
        &self.events
    }

    pub fn add_event(&mut self, event: CalendarEvent) -> Result<(), String> {
        let conflicts = self.find_conflicts(&event);
        if !conflicts.is_empty() {
            let conflict_titles: Vec<String> = conflicts.iter().map(|e| e.summary.clone()).collect();
            return Err(format!(
                "Conflit d'agenda détecté avec les événements existants : {}",
                conflict_titles.join(", ")
            ));
        }
        self.events.push(event);
        Ok(())
    }

    pub fn find_conflicts(&self, candidate: &CalendarEvent) -> Vec<&CalendarEvent> {
        self.events
            .iter()
            .filter(|e| {
                if e.uid == candidate.uid {
                    return false;
                }
                // Overlap condition: candidate.start < e.end && candidate.end > e.start
                candidate.start_time < e.end_time && candidate.end_time > e.start_time
            })
            .collect()
    }

    pub fn events_for_date(&self, date_prefix: &str) -> Vec<&CalendarEvent> {
        self.events
            .iter()
            .filter(|e| e.start_time.starts_with(date_prefix))
            .collect()
    }

    /// Synchronizes scheduled events into Annora's sovereign knowledge graph as structured triplets
    pub fn sync_with_memory_graph(&self, graph: &mut KnowledgeGraph) {
        for event in &self.events {
            let event_node = NodeId(uuid::Uuid::now_v7());
            let date_pred = PredicateId(uuid::Uuid::now_v7());

            let triplet = Triplet {
                id: TripletId(uuid::Uuid::now_v7()),
                subject: event_node,
                predicate: date_pred,
                object: NodeValue::Text(format!(
                    "Événement '{}' prévu le {} (Lieu: {})",
                    event.summary,
                    event.start_time,
                    event.location.as_deref().unwrap_or("Non spécifié")
                )),
                metadata: TripletMetadata {
                    weight: 1.0,
                    created_at: Timestamp::now(),
                    last_activated: Timestamp::now(),
                    source: TripletSource::User,
                    confidence: 1.0,
                    context: ContextPartition::Personal,
                    trust_level: TrustLevel::User,
                    decay_rate: 0.005,
                },
            };
            graph.insert(triplet);
        }
    }

    /// Unfolds RFC 5545 lines (CRLF + space/tab continuation)
    fn unfold_ics_lines(content: &str) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for raw in content.lines() {
            let line = raw.trim_end_matches('\r');
            if line.starts_with(' ') || line.starts_with('\t') {
                if let Some(last) = lines.last_mut() {
                    last.push_str(&line[1..]);
                } else {
                    lines.push(line.trim_start().to_string());
                }
            } else if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }

    /// Parses an RFC 5545 property name and value, stripping parameters like `;TZID=...` or `;VALUE=DATE`
    fn parse_property(line: &str) -> Option<(&str, &str)> {
        let colon = line.find(':')?;
        let name_part = &line[..colon];
        let val = &line[colon + 1..];
        let name = name_part.split(';').next()?.trim();
        Some((name, val))
    }

    /// Parse RFC 5545 iCalendar format into structured events, supporting line folding, parameters, and RRULE
    pub fn parse_ics(content: &str) -> Result<Vec<CalendarEvent>, String> {
        let mut events = Vec::new();
        let mut in_vevent = false;
        let mut current_uid = String::new();
        let mut current_summary = String::new();
        let mut current_desc = String::new();
        let mut current_start = String::new();
        let mut current_end = String::new();
        let mut current_location = None;
        let mut current_recurrence = None;
        let mut current_status = "CONFIRMED".to_string();

        let unfolded = Self::unfold_ics_lines(content);

        for line in unfolded {
            if line == "BEGIN:VEVENT" {
                in_vevent = true;
                current_uid.clear();
                current_summary.clear();
                current_desc.clear();
                current_start.clear();
                current_end.clear();
                current_location = None;
                current_recurrence = None;
                current_status = "CONFIRMED".to_string();
            } else if line == "END:VEVENT" {
                if in_vevent {
                    if !current_summary.is_empty() {
                        events.push(CalendarEvent {
                            uid: if current_uid.is_empty() { format!("uid-{}", uuid::Uuid::now_v7()) } else { current_uid.clone() },
                            summary: current_summary.clone(),
                            description: current_desc.clone(),
                            start_time: current_start.clone(),
                            end_time: current_end.clone(),
                            location: current_location.clone(),
                            recurrence: current_recurrence.clone(),
                            status: current_status.clone(),
                        });
                    }
                    in_vevent = false;
                }
            } else if in_vevent {
                if let Some((name, val)) = Self::parse_property(&line) {
                    match name.to_uppercase().as_str() {
                        "UID" => current_uid = val.to_string(),
                        "SUMMARY" => current_summary = val.to_string(),
                        "DESCRIPTION" => current_desc = val.to_string(),
                        "DTSTART" => current_start = val.to_string(),
                        "DTEND" => current_end = val.to_string(),
                        "LOCATION" => current_location = Some(val.to_string()),
                        "RRULE" => current_recurrence = Some(val.to_string()),
                        "STATUS" => current_status = val.to_string(),
                        _ => {}
                    }
                }
            }
        }

        Ok(events)
    }

    /// Export events to RFC 5545 iCalendar string
    pub fn export_ics(events: &[CalendarEvent]) -> String {
        let mut ics = String::new();
        ics.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Annora Cognitive OS//FR\r\n");
        for e in events {
            ics.push_str("BEGIN:VEVENT\r\n");
            ics.push_str(&format!("UID:{}\r\n", e.uid));
            ics.push_str(&format!("SUMMARY:{}\r\n", e.summary));
            ics.push_str(&format!("DESCRIPTION:{}\r\n", e.description));
            ics.push_str(&format!("DTSTART:{}\r\n", e.start_time));
            ics.push_str(&format!("DTEND:{}\r\n", e.end_time));
            if let Some(ref loc) = e.location {
                ics.push_str(&format!("LOCATION:{}\r\n", loc));
            }
            if let Some(ref rrule) = e.recurrence {
                ics.push_str(&format!("RRULE:{}\r\n", rrule));
            }
            ics.push_str(&format!("STATUS:{}\r\n", e.status));
            ics.push_str("END:VEVENT\r\n");
        }
        ics.push_str("END:VCALENDAR\r\n");
        ics
    }
}

impl Default for CalendarManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn check_calendar_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Filesystem, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Write),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calendar_conflict_detection() {
        let mut mgr = CalendarManager::new();
        assert_eq!(mgr.events().len(), 2);

        // Conflicting event at 10:30 (overlaps with 10:00-11:30)
        let conflict_event = CalendarEvent {
            uid: "evt-conflict".to_string(),
            summary: "Réunion concurrente".to_string(),
            description: "".to_string(),
            start_time: "2026-10-09T10:30:00Z".to_string(),
            end_time: "2026-10-09T11:00:00Z".to_string(),
            location: None,
            recurrence: None,
            status: "CONFIRMED".to_string(),
        };

        let conflicts = mgr.find_conflicts(&conflict_event);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].uid, "evt-20261009-01");

        // Non-conflicting event at 15:00
        let ok_event = CalendarEvent {
            uid: "evt-ok".to_string(),
            summary: "Session libre".to_string(),
            description: "".to_string(),
            start_time: "2026-10-09T15:00:00Z".to_string(),
            end_time: "2026-10-09T16:00:00Z".to_string(),
            location: None,
            recurrence: None,
            status: "CONFIRMED".to_string(),
        };
        assert!(mgr.add_event(ok_event).is_ok());
        assert_eq!(mgr.events().len(), 3);
    }

    #[test]
    fn test_ics_parsing_and_export_roundtrip() {
        let sample_ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:test-123\r\nSUMMARY:Sprint Review\r\nDESCRIPTION:Démonstration Annora HUD\r\nDTSTART:20261009T140000Z\r\nDTEND:20261009T150000Z\r\nLOCATION:Bureau 42\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = CalendarManager::parse_ics(sample_ics).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].summary, "Sprint Review");
        assert_eq!(parsed[0].location.as_deref(), Some("Bureau 42"));

        let exported = CalendarManager::export_ics(&parsed);
        assert!(exported.contains("SUMMARY:Sprint Review"));
        assert!(exported.contains("UID:test-123"));
    }

    #[test]
    fn test_sync_with_memory_graph() {
        let mgr = CalendarManager::new();
        let mut graph = KnowledgeGraph::new();
        mgr.sync_with_memory_graph(&mut graph);

        assert_eq!(graph.len(), 2);
        let recalled = graph.search_text("Architecture");
        assert_eq!(recalled.len(), 1);
    }

    #[test]
    fn test_ics_folded_lines_and_recurrence_rules() {
        let folded_ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:recurring-001\r\n\
SUMMARY;LANGUAGE=fr:Revue Hebdomadaire \r\n\
\x20Annora OS\r\n\
DESCRIPTION:Discussion technique et \r\n\
\x20souveraine sur le noyau Rust.\r\n\
DTSTART;TZID=Europe/Paris:20261012T090000\r\n\
DTEND;TZID=Europe/Paris:20261012T100000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=10\r\n\
LOCATION;ALTREP=\"salle-virtuelle\":Salle Conférence\r\n\
STATUS:CONFIRMED\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let parsed = CalendarManager::parse_ics(folded_ics).unwrap();
        assert_eq!(parsed.len(), 1);
        let ev = &parsed[0];
        assert_eq!(ev.uid, "recurring-001");
        assert_eq!(ev.summary, "Revue Hebdomadaire Annora OS", "Folded summary must be unfolded");
        assert_eq!(ev.description, "Discussion technique et souveraine sur le noyau Rust.", "Folded description must be unfolded");
        assert_eq!(ev.start_time, "20261012T090000", "Parameterized DTSTART must be parsed");
        assert_eq!(ev.end_time, "20261012T100000", "Parameterized DTEND must be parsed");
        assert_eq!(ev.location.as_deref(), Some("Salle Conférence"));
        assert_eq!(ev.recurrence.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO;COUNT=10"));
        assert_eq!(ev.status, "CONFIRMED");

        // Roundtrip export must contain RRULE
        let exported = CalendarManager::export_ics(&parsed);
        assert!(exported.contains("RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=10"));
    }
}

