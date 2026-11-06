//! Sous-ensemble iCalendar explicite : aucune conversion implicite de fuseau.
use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

use crate::CalendarEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TimeKind {
    Utc,
    Floating,
    Date,
    Zoned(String),
}

pub(crate) struct EventRange {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub kind: TimeKind,
    recurring: bool,
}

impl EventRange {
    pub fn ensure_schedulable(&self) -> Result<(), String> {
        if matches!(self.kind, TimeKind::Zoned(_)) {
            return Err("TZID conservé mais non résolu : un moteur de fuseaux est nécessaire pour calculer les conflits".into());
        }
        if self.recurring {
            return Err("Récurrence conservée mais non développée : calcul de conflits indisponible pour RRULE".into());
        }
        Ok(())
    }
}

fn parse_time(raw: &str, timezone: Option<&str>) -> Result<(NaiveDateTime, TimeKind), String> {
    if let Some(timezone) = timezone {
        if timezone.is_empty() || timezone.contains(['\r', '\n', '"', ';']) {
            return Err("TZID invalide".into());
        }
        let time = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%S")
            .map_err(|_| "TZID exige une date-heure locale iCalendar sans suffixe Z")?;
        if time.nanosecond() != 0 {
            return Err("Secondes intercalaires non prises en charge".into());
        }
        if time.format("%Y%m%dT%H%M%S").to_string() != raw {
            return Err("Date-heure locale iCalendar non canonique".into());
        }
        return Ok((time, TimeKind::Zoned(timezone.to_string())));
    }
    if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()) {
        return NaiveDate::parse_from_str(raw, "%Y%m%d")
            .map(|date| (date.and_hms_opt(0, 0, 0).unwrap(), TimeKind::Date))
            .map_err(|_| "Date iCalendar invalide".into());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        if dt.nanosecond() != 0 {
            return Err(
                "Précision inférieure à la seconde ou seconde intercalaire non prise en charge"
                    .into(),
            );
        }
        return Ok((dt.naive_utc(), TimeKind::Utc));
    }
    let (core, kind) = match raw.strip_suffix('Z') {
        Some(core) => (core, TimeKind::Utc),
        None => (raw, TimeKind::Floating),
    };
    let time = NaiveDateTime::parse_from_str(core, "%Y%m%dT%H%M%S")
        .map_err(|_| "Date-heure invalide : utiliser iCalendar basique ou RFC 3339")?;
    if time.nanosecond() != 0 {
        return Err("Secondes intercalaires non prises en charge".into());
    }
    if time.format("%Y%m%dT%H%M%S").to_string() != core {
        return Err("Date-heure iCalendar non canonique".into());
    }
    Ok((time, kind))
}

pub(crate) fn range(event: &CalendarEvent) -> Result<EventRange, String> {
    if !matches!(
        event.status.as_str(),
        "CONFIRMED" | "TENTATIVE" | "CANCELLED"
    ) {
        return Err("STATUS invalide".into());
    }
    let (start, kind) = parse_time(&event.start_time, event.start_timezone.as_deref())?;
    let end = if event.end_time.is_empty() {
        if event.end_timezone.is_some() {
            return Err("Fuseau de fin sans DTEND".into());
        }
        if kind == TimeKind::Date {
            start
                .checked_add_signed(Duration::days(1))
                .ok_or("Date hors limites")?
        } else {
            start
        }
    } else {
        let (end, end_kind) = parse_time(&event.end_time, event.end_timezone.as_deref())?;
        if kind != end_kind {
            return Err("DTSTART et DTEND doivent partager le même type et le même fuseau".into());
        }
        end
    };
    if !(0..=9999).contains(&start.year()) || !(0..=9999).contains(&end.year()) {
        return Err("Année hors des quatre chiffres iCalendar".into());
    }
    if end < start || (!event.end_time.is_empty() && end == start) {
        return Err("DTEND doit suivre DTSTART (fin exclusive pour les journées entières)".into());
    }
    Ok(EventRange {
        start,
        end,
        kind,
        recurring: event.recurrence.is_some(),
    })
}

fn format_time(value: NaiveDateTime, kind: &TimeKind) -> String {
    value
        .format(match kind {
            TimeKind::Date => "%Y%m%d",
            TimeKind::Utc => "%Y%m%dT%H%M%SZ",
            _ => "%Y%m%dT%H%M%S",
        })
        .to_string()
}

fn text_decode(value: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('n' | 'N') => '\n',
            Some(c @ ('\\' | ',' | ';')) => c,
            _ => return Err("Échappement TEXT iCalendar invalide".into()),
        });
    }
    Ok(out)
}

fn text_encode(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

/// Une ligne physique fait au plus 75 octets, sans couper un caractère UTF-8.
fn push_line(out: &mut String, line: &str) {
    let mut bytes = 0;
    for ch in line.chars() {
        if bytes + ch.len_utf8() > 75 {
            out.push_str("\r\n ");
            bytes = 1;
        }
        out.push(ch);
        bytes += ch.len_utf8();
    }
    out.push_str("\r\n");
}

fn unfold(content: &str) -> Result<Vec<String>, String> {
    let mut lines: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.starts_with([' ', '\t']) {
            lines
                .last_mut()
                .ok_or("Continuation iCalendar sans ligne précédente")?
                .push_str(&line[1..]);
        } else if !line.is_empty() {
            lines.push(line.to_string());
        }
    }
    Ok(lines)
}

struct Property {
    value: String,
    params: HashMap<String, String>,
}

/// Les séparateurs dans une valeur de paramètre entre guillemets restent littéraux.
fn property(line: &str) -> Result<(String, Property), String> {
    let mut quoted = false;
    let colon = line
        .char_indices()
        .find_map(|(i, ch)| {
            if ch == '"' {
                quoted = !quoted;
            }
            (ch == ':' && !quoted).then_some(i)
        })
        .ok_or("Propriété iCalendar sans séparateur ':'")?;
    let mut parts = Vec::new();
    let mut begin = 0;
    quoted = false;
    for (i, ch) in line[..colon].char_indices() {
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == ';' && !quoted {
            parts.push(&line[begin..i]);
            begin = i + 1;
        }
    }
    parts.push(&line[begin..colon]);
    let name = parts[0].to_ascii_uppercase();
    let mut params = HashMap::new();
    for raw in &parts[1..] {
        let (key, value) = raw.split_once('=').ok_or("Paramètre iCalendar sans '='")?;
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if params
            .insert(key.to_ascii_uppercase(), value.to_string())
            .is_some()
        {
            return Err("Paramètre iCalendar dupliqué".into());
        }
    }
    Ok((
        name,
        Property {
            value: line[colon + 1..].to_string(),
            params,
        },
    ))
}

fn read_date(property: &Property) -> Result<(String, Option<String>), String> {
    let timezone = property.params.get("TZID").cloned();
    let (time, kind) = parse_time(&property.value, timezone.as_deref())?;
    let value_type = property.params.get("VALUE").map(|v| v.to_ascii_uppercase());
    match value_type.as_deref() {
        Some("DATE") if kind == TimeKind::Date => {}
        None | Some("DATE-TIME") if kind != TimeKind::Date => {}
        _ => return Err("VALUE=DATE exige une date ; DATE-TIME exige une date-heure".into()),
    }
    Ok((format_time(time, &kind), timezone))
}

fn duration(value: &str) -> Result<Duration, String> {
    let value = value
        .strip_prefix('+')
        .unwrap_or(value)
        .strip_prefix('P')
        .ok_or("DURATION doit être positive et commencer par P")?;
    let mut seconds = 0i64;
    let mut digits = String::new();
    let mut in_time = false;
    let mut rank = 0;
    let mut count = 0;
    let mut time_count = 0;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if ch == 'T' && !in_time && digits.is_empty() && rank != 1 {
            in_time = true;
            continue;
        }
        let (next_rank, multiplier) = match (in_time, ch) {
            (false, 'W') if count == 0 => (1, 604800),
            (false, 'D') if rank != 1 => (2, 86400),
            (true, 'H') => (3, 3600),
            (true, 'M') => (4, 60),
            (true, 'S') => (5, 1),
            _ => return Err("Unité DURATION invalide ou non prise en charge".into()),
        };
        if next_rank <= rank {
            return Err("Ordre des unités DURATION invalide".into());
        }
        let n: i64 = digits.parse().map_err(|_| "Nombre DURATION invalide")?;
        seconds = seconds
            .checked_add(n.checked_mul(multiplier).ok_or("DURATION trop grande")?)
            .ok_or("DURATION trop grande")?;
        digits.clear();
        rank = next_rank;
        count += 1;
        if in_time {
            time_count += 1;
        }
    }
    if !digits.is_empty() || count == 0 || seconds == 0 || (in_time && time_count == 0) {
        return Err("DURATION vide, nulle ou incomplète".into());
    }
    Duration::try_seconds(seconds).ok_or("DURATION trop grande".into())
}

fn finish(mut properties: HashMap<String, Property>) -> Result<CalendarEvent, String> {
    let required = |properties: &mut HashMap<String, Property>, name: &str| {
        properties
            .remove(name)
            .ok_or_else(|| format!("VEVENT sans {name}"))
    };
    let uid = text_decode(&required(&mut properties, "UID")?.value)?;
    if uid.is_empty() {
        return Err("UID vide".into());
    }
    let (start_time, start_timezone) = read_date(&required(&mut properties, "DTSTART")?)?;
    let (mut end_time, mut end_timezone) = match properties.remove("DTEND") {
        Some(value) => read_date(&value)?,
        None => (String::new(), None),
    };
    if let Some(value) = properties.remove("DURATION") {
        if !end_time.is_empty() {
            return Err("DTEND et DURATION sont mutuellement exclusifs".into());
        }
        let (start, kind) = parse_time(&start_time, start_timezone.as_deref())?;
        if matches!(kind, TimeKind::Zoned(_)) {
            return Err(
                "DURATION avec TZID nécessite un moteur de fuseaux pour les changements d'heure"
                    .into(),
            );
        }
        let span = duration(&value.value)?;
        if kind == TimeKind::Date && value.value.contains('T') {
            return Err("Une journée entière exige une durée en jours ou semaines".into());
        }
        end_time = format_time(
            start.checked_add_signed(span).ok_or("Fin hors limites")?,
            &kind,
        );
        end_timezone = start_timezone.clone();
    }
    let mut text = |name: &str| {
        properties
            .remove(name)
            .map(|p| text_decode(&p.value))
            .transpose()
    };
    let summary = text("SUMMARY")?.unwrap_or_default();
    let description = text("DESCRIPTION")?.unwrap_or_default();
    let location = text("LOCATION")?;
    let mut event = CalendarEvent {
        uid,
        summary,
        description,
        start_time,
        end_time,
        start_timezone,
        end_timezone,
        location,
        recurrence: properties.remove("RRULE").map(|p| p.value),
        status: properties
            .remove("STATUS")
            .map(|p| p.value.to_ascii_uppercase())
            .unwrap_or_else(|| "CONFIRMED".into()),
    };
    if !matches!(
        event.status.as_str(),
        "CONFIRMED" | "TENTATIVE" | "CANCELLED"
    ) {
        return Err("STATUS invalide".into());
    }
    if event
        .recurrence
        .as_ref()
        .is_some_and(|rule| rule.is_empty())
    {
        return Err("RRULE vide".into());
    }
    let checked = range(&event)?;
    if event.end_time.is_empty() && checked.kind == TimeKind::Date {
        event.end_time = format_time(checked.end, &checked.kind);
    }
    Ok(event)
}

pub(crate) fn parse(content: &str) -> Result<Vec<CalendarEvent>, String> {
    let mut events = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut event = None::<HashMap<String, Property>>;
    let mut calendar_seen = false;
    for line in unfold(content)? {
        let (name, value) = property(&line)?;
        if name == "BEGIN" {
            let component = value.value.to_ascii_uppercase();
            match (component.as_str(), stack.last().map(String::as_str)) {
                ("VCALENDAR", None) if !calendar_seen => {
                    calendar_seen = true;
                }
                ("VEVENT", Some("VCALENDAR")) => {
                    event = Some(HashMap::new());
                }
                _ => {
                    return Err(format!(
                        "Composant non pris en charge ou mal imbriqué : {component}"
                    ));
                }
            }
            stack.push(component);
        } else if name == "END" {
            let component = value.value.to_ascii_uppercase();
            if stack.pop().as_deref() != Some(component.as_str()) {
                return Err("Fin de composant iCalendar incohérente".into());
            }
            if component == "VEVENT" {
                events.push(finish(event.take().ok_or("VEVENT absent")?)?);
            }
        } else if stack.last().is_none() {
            return Err("Propriété hors calendrier".into());
        } else if stack.last().map(String::as_str) == Some("VCALENDAR") {
            if name == "METHOD" && !value.value.eq_ignore_ascii_case("PUBLISH") {
                return Err("Méthode iTIP non prise en charge : importer un calendrier publié, pas une invitation ou annulation".into());
            }
            if name == "VERSION" && value.value != "2.0" {
                return Err("Seule la version iCalendar 2.0 est prise en charge".into());
            }
        } else if stack.last().map(String::as_str) == Some("VEVENT") {
            if name == "TRANSP" && !value.value.eq_ignore_ascii_case("OPAQUE") {
                return Err(
                    "TRANSP non opaque non pris en charge : refus plutôt qu'un faux créneau occupé"
                        .into(),
                );
            }
            if matches!(name.as_str(), "RDATE" | "EXDATE" | "RECURRENCE-ID") {
                return Err(format!(
                    "{name} non pris en charge : exceptions de récurrence non importées"
                ));
            }
            if matches!(
                name.as_str(),
                "UID"
                    | "SUMMARY"
                    | "DESCRIPTION"
                    | "DTSTART"
                    | "DTEND"
                    | "DURATION"
                    | "LOCATION"
                    | "RRULE"
                    | "STATUS"
            ) && event
                .as_mut()
                .ok_or("VEVENT absent")?
                .insert(name.clone(), value)
                .is_some()
            {
                return Err(format!("Propriété VEVENT dupliquée : {name}"));
            }
        }
    }
    if !calendar_seen || !stack.is_empty() {
        return Err("Calendrier absent ou incomplet".into());
    }
    Ok(events)
}

pub(crate) fn export(events: &[CalendarEvent]) -> Result<String, String> {
    let mut out =
        String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Hakille Cognitive OS//FR\r\n");
    for event in events {
        if event.uid.is_empty() {
            return Err("UID vide".into());
        }
        let checked = range(event)?;
        push_line(&mut out, "BEGIN:VEVENT");
        // Estampille réelle de l'export, sans prétendre conserver la date source.
        push_line(
            &mut out,
            &format!("DTSTAMP:{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ")),
        );
        for (name, value) in [
            ("UID", &event.uid),
            ("SUMMARY", &event.summary),
            ("DESCRIPTION", &event.description),
        ] {
            push_line(&mut out, &format!("{name}:{}", text_encode(value)));
        }
        let parameter = match &checked.kind {
            TimeKind::Date => ";VALUE=DATE".to_string(),
            TimeKind::Zoned(tz) => format!(";TZID=\"{tz}\""),
            _ => String::new(),
        };
        for (name, time) in [("DTSTART", checked.start), ("DTEND", checked.end)] {
            if name == "DTEND" && checked.start == checked.end {
                continue; // Un instant sans durée ne doit pas exporter un DTEND égal au début.
            }
            push_line(
                &mut out,
                &format!("{name}{parameter}:{}", format_time(time, &checked.kind)),
            );
        }
        if let Some(location) = &event.location {
            push_line(&mut out, &format!("LOCATION:{}", text_encode(location)));
        }
        if let Some(rule) = &event.recurrence {
            if rule.contains(['\r', '\n']) || rule.is_empty() {
                return Err("RRULE invalide".into());
            }
            push_line(&mut out, &format!("RRULE:{rule}"));
        }
        if !matches!(
            event.status.as_str(),
            "CONFIRMED" | "TENTATIVE" | "CANCELLED"
        ) {
            return Err("STATUS invalide".into());
        }
        push_line(&mut out, &format!("STATUS:{}", event.status));
        push_line(&mut out, "END:VEVENT");
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}
