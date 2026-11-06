# Agent calendrier : bibliothèque locale

Cette crate expose des fonctions de calendrier locales. Elle n'implique ni une
connexion CalDAV, ni une intégration OS, ni l'exécution d'un agent métier WASM.
`CalendarManager::new()` et `default()` créent un agenda vide ; les événements
fictifs historiques sont disponibles uniquement via `CalendarManager::demo()`.

## Import et export iCalendar

`parse_ics` importe un **sous-ensemble** RFC 5545 et retourne les événements sans
modifier l'agenda. `export_ics` retourne désormais `Result<String, String>` : une
donnée invalide produit une erreur, jamais une date corrigée silencieusement.

Fonctions prises en charge :

- Dates-heures UTC, heures flottantes et journées entières `VALUE=DATE`, avec
  validation calendaire stricte. Les entrées RFC 3339 avec décalage numérique
  sont normalisées en UTC lors de l'export.
- Fin exclusive `DTEND`. Sans fin, une journée dure un jour ; un rendez-vous
  horaire sans durée reste un instant, sans bloquer un créneau.
- `DURATION` positive en semaines/jours ou heures/minutes/secondes pour les
  événements sans `TZID`. `DTEND` et `DURATION` ensemble sont refusés.
- `TZID` conservé dans `start_timezone` et `end_timezone` (champs JSON optionnels
  avec valeur par défaut `None`). Aucune conversion de fuseau n'est inventée.
- `UID`, `SUMMARY`, `DESCRIPTION`, `LOCATION`, `RRULE` et `STATUS` conservés.
  Les textes échappés (`\\`, `\n`, `\,`, `\;`), paramètres entre guillemets
  contenant `:` et lignes repliées sont correctement lus.
- Export avec `DTSTAMP` à l'instant UTC réel d'export (l'estampille du fichier
  source n'est pas conservée), CRLF et repli à 75 octets maximum sans couper UTF-8. Les sauts de
  ligne des textes deviennent des échappements, pas des propriétés injectées.

Limites explicites : les règles `RRULE` sont conservées mais pas développées.
Les définitions `VTIMEZONE`, alarmes `VALARM` et exceptions de récurrence
`RDATE`, `EXDATE`, `RECURRENCE-ID` sont refusées. Les événements transparents
`TRANSP:TRANSPARENT` et méthodes iTIP d'invitation/annulation sont aussi refusés
pour éviter de les interpréter comme des plages occupées publiées.
Les propriétés supplémentaires comme `ATTENDEE`, `ORGANIZER` et les paramètres de présentation ne sont
pas stockés : ceci n'est **pas** un roundtrip intégral d'un agenda externe.
Les fractions de seconde non nulles et secondes intercalaires sont refusées.

## Planification

`find_conflicts` retourne maintenant `Result<Vec<&CalendarEvent>, String>`.
Il compare les intervalles `[début, fin)` et accepte des créneaux adjacents.
Les événements annulés et les instants sans durée ne bloquent pas la plage.
Les bases UTC, flottantes et journées entières doivent être résolues explicitement
avant de les mélanger ; `TZID` et `RRULE` exigent un moteur dédié et produisent
une erreur de planification. Un résultat vide ne cache donc pas une conversion
manquante.

`add_event` refuse les UID vides/dupliqués et les conflits. `import_ics` valide
l'intégralité du lot dans une copie avant de modifier l'agenda : en cas de
conflit, d'UID dupliqué ou de temporalité non résolue, aucun préfixe du lot n'est
importé. `events_for_date` retrouve aussi les événements qui ont commencé un
jour précédent, en respectant la fin exclusive ; le jour est UTC pour les
instants UTC et civil pour les événements flottants ou journées entières.

```rust
use agent_calendar::CalendarManager;

let mut calendar = CalendarManager::new();
let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:conge-1\r\nSUMMARY:Congé\r\nDTSTART;VALUE=DATE:20261103\r\nDURATION:P2D\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
calendar.import_ics(input).expect("import valide");
assert_eq!(calendar.events_for_date("2026-11-04").len(), 1);
assert!(calendar.events_for_date("2026-11-05").is_empty());
let exported = CalendarManager::export_ics(calendar.events()).expect("export valide");
assert!(exported.contains("DTEND;VALUE=DATE:20261105"));
```

Validation : `cargo test -p agent-calendar --locked`.
