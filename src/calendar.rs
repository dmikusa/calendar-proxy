use std::io::BufReader;

#[derive(Debug, Clone)]
pub struct SanitizedEvent {
    pub uid: String,
    pub dtstart: String,
    pub dtend: Option<String>,
    pub duration: Option<String>,
    pub rrule: Option<String>,
    pub exdate: Vec<String>,
    pub rdate: Vec<String>,
    pub recurrence_id: Option<String>,
    pub transp: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SanitizedCalendar {
    pub vtimezones: Vec<String>,
    pub events: Vec<SanitizedEvent>,
}

impl SanitizedCalendar {
    pub fn new() -> Self {
        Self {
            vtimezones: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: SanitizedCalendar) {
        for tz in other.vtimezones {
            let tzid = extract_tzid(&tz);
            if !self
                .vtimezones
                .iter()
                .any(|existing| extract_tzid(existing) == tzid)
            {
                self.vtimezones.push(tz);
            }
        }
        for event in other.events {
            if !self.events.iter().any(|e| e.uid == event.uid) {
                self.events.push(event);
            }
        }
    }

    pub fn to_ics_string(&self) -> String {
        let mut out = String::new();
        out.push_str("BEGIN:VCALENDAR\r\n");
        out.push_str("VERSION:2.0\r\n");
        out.push_str("PRODID:-//Calendar Proxy//EN\r\n");
        out.push_str("CALSCALE:GREGORIAN\r\n");
        out.push_str("METHOD:PUBLISH\r\n");

        for tz in &self.vtimezones {
            out.push_str(tz);
            out.push_str("\r\n");
        }

        for event in &self.events {
            out.push_str("BEGIN:VEVENT\r\n");
            out.push_str(&event.uid);
            out.push_str("\r\n");
            out.push_str(&event.dtstart);
            out.push_str("\r\n");
            if let Some(ref dtend) = event.dtend {
                out.push_str(dtend);
                out.push_str("\r\n");
            }
            if let Some(ref dur) = event.duration {
                out.push_str(dur);
                out.push_str("\r\n");
            }
            if let Some(ref rrule) = event.rrule {
                out.push_str(rrule);
                out.push_str("\r\n");
            }
            for ex in &event.exdate {
                out.push_str(ex);
                out.push_str("\r\n");
            }
            for rd in &event.rdate {
                out.push_str(rd);
                out.push_str("\r\n");
            }
            if let Some(ref rid) = event.recurrence_id {
                out.push_str(rid);
                out.push_str("\r\n");
            }
            if let Some(ref t) = event.transp {
                out.push_str(t);
                out.push_str("\r\n");
            }
            if let Some(ref s) = event.status {
                out.push_str(s);
                out.push_str("\r\n");
            }
            out.push_str("SUMMARY:Busy\r\n");
            out.push_str("END:VEVENT\r\n");
        }

        out.push_str("END:VCALENDAR\r\n");
        out
    }
}

impl Default for SanitizedCalendar {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_tzid(vtimezone_block: &str) -> String {
    for line in vtimezone_block.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("TZID:") {
            return trimmed.trim_start_matches("TZID:").trim().to_string();
        }
    }
    String::new()
}

/// Represents a parsed ICS property with name, params, and value.
#[derive(Debug, Clone)]
pub struct IcalProperty {
    pub name: String,
    pub params: Vec<(String, Vec<String>)>,
    pub value: String,
}

/// Convert an ical::Property into our IcalProperty type.
fn convert_property(prop: &ical::property::Property) -> IcalProperty {
    let name = prop.name.to_uppercase();
    let params = prop
        .params
        .as_ref()
        .map(|p| {
            p.iter()
                .map(|(k, v)| (k.to_uppercase(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    let value = prop.value.clone().unwrap_or_default();
    IcalProperty {
        name,
        params,
        value,
    }
}

/// Serialize an IcalProperty back to ICS property string.
pub fn property_to_string(prop: &IcalProperty) -> String {
    let mut out = prop.name.clone();
    for (key, vals) in &prop.params {
        out.push(';');
        out.push_str(key);
        out.push('=');
        out.push_str(&vals.join(","));
    }
    out.push(':');
    out.push_str(&prop.value);
    out
}

/// Check if an event has STATUS:CANCELLED.
pub fn is_cancelled(props: &[IcalProperty]) -> bool {
    props
        .iter()
        .any(|p| p.name == "STATUS" && p.value.to_uppercase() == "CANCELLED")
}

/// Convert a Vec of IcalProperty into a SanitizedEvent (only allowed fields).
fn properties_to_sanitized_event(props: &[IcalProperty]) -> Option<SanitizedEvent> {
    let uid = props
        .iter()
        .find(|p| p.name == "UID")
        .map(property_to_string)?;

    let dtstart = props
        .iter()
        .find(|p| p.name == "DTSTART")
        .map(property_to_string)?;

    let dtend = props
        .iter()
        .find(|p| p.name == "DTEND")
        .map(property_to_string);

    let duration = props
        .iter()
        .find(|p| p.name == "DURATION")
        .map(property_to_string);

    let rrule = props
        .iter()
        .find(|p| p.name == "RRULE")
        .map(property_to_string);

    let exdate: Vec<String> = props
        .iter()
        .filter(|p| p.name == "EXDATE")
        .map(property_to_string)
        .collect();

    let rdate: Vec<String> = props
        .iter()
        .filter(|p| p.name == "RDATE")
        .map(property_to_string)
        .collect();

    let recurrence_id = props
        .iter()
        .find(|p| p.name == "RECURRENCE-ID")
        .map(property_to_string);

    let transp = props
        .iter()
        .find(|p| p.name == "TRANSP")
        .map(property_to_string);

    let status = props
        .iter()
        .find(|p| p.name == "STATUS")
        .map(property_to_string);

    Some(SanitizedEvent {
        uid,
        dtstart,
        dtend,
        duration,
        rrule,
        exdate,
        rdate,
        recurrence_id,
        transp,
        status,
    })
}

/// Extract raw VTIMEZONE blocks from the source ICS content.
fn extract_vtimezones(content: &str) -> Vec<String> {
    let mut zones = Vec::new();
    let mut pos = 0;
    let upper = content.to_uppercase();
    while let Some(begin) = upper[pos..].find("BEGIN:VTIMEZONE") {
        let begin = pos + begin;
        if let Some(end) = upper[begin..].find("END:VTIMEZONE") {
            let end = begin + end + "END:VTIMEZONE".len();
            zones.push(content[begin..end].to_string());
            pos = end;
        } else {
            break;
        }
    }
    zones
}

/// Parse an ICS string into a SanitizedCalendar.
pub fn parse_ics(content: &str) -> Result<SanitizedCalendar, String> {
    let reader = BufReader::new(std::io::Cursor::new(content));
    let parser = ical::IcalParser::new(reader);
    let mut calendar = SanitizedCalendar::new();

    // Extract VTIMEZONE blocks from raw content
    calendar.vtimezones = extract_vtimezones(content);

    for component_result in parser {
        let ical_cal = component_result.map_err(|e| format!("ICS parse error: {e}"))?;

        for event in &ical_cal.events {
            let props: Vec<IcalProperty> = event.properties.iter().map(convert_property).collect();

            if is_cancelled(&props) {
                continue;
            }

            if let Some(sanitized) = properties_to_sanitized_event(&props) {
                calendar.events.push(sanitized);
            }
        }
    }

    Ok(calendar)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture(name: &str) -> String {
        let path = format!("tests/fixtures/{name}.ics");
        std::fs::read_to_string(&path).expect("fixture not found")
    }

    #[test]
    fn test_simple_event_sanitization() {
        let content = load_fixture("simple");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 1);
        let event = &cal.events[0];
        assert!(event.uid.contains("test-uid-1"));
        assert!(event.dtstart.contains("DTSTART"));
        assert!(event.dtstart.contains("20240601T090000Z"));
        assert!(event.dtend.is_some());
        assert!(event.dtend.as_ref().unwrap().contains("20240601T100000Z"));
        assert!(event.transp.is_some());
        assert!(event.transp.as_ref().unwrap().contains("OPAQUE"));
        assert!(event.status.is_some());
        assert!(event.status.as_ref().unwrap().contains("CONFIRMED"));
        // SUMMARY, DESCRIPTION, LOCATION, ORGANIZER, ATTENDEE should not be kept
        // (they are not in the whitelist)
    }

    #[test]
    fn test_output_has_summary_busy() {
        let content = load_fixture("simple");
        let cal = parse_ics(&content).unwrap();
        let output = cal.to_ics_string();
        assert!(output.contains("SUMMARY:Busy"));
        assert!(output.contains("BEGIN:VCALENDAR"));
        assert!(output.contains("END:VCALENDAR"));
        assert!(output.contains("BEGIN:VEVENT"));
        assert!(output.contains("END:VEVENT"));
        // Original summary should not appear
        assert!(!output.contains("Team Standup"));
        assert!(!output.contains("Conference Room A"));
        assert!(!output.contains("alice@example.com"));
    }

    #[test]
    fn test_cancelled_event_removed() {
        let content = load_fixture("cancelled");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 0);
    }

    #[test]
    fn test_all_day_event() {
        let content = load_fixture("all_day");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 1);
        let event = &cal.events[0];
        assert!(event.dtstart.contains("VALUE=DATE"));
        assert!(event.dtstart.contains("20240601"));
        assert!(event.transp.is_some());
        assert!(event.transp.as_ref().unwrap().contains("TRANSPARENT"));
    }

    #[test]
    fn test_recurring_event() {
        let content = load_fixture("recurring");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 1);
        let event = &cal.events[0];
        assert!(event.rrule.is_some());
        assert_eq!(event.exdate.len(), 1);
        assert!(event.dtstart.contains("TZID=America/New_York"));
    }

    #[test]
    fn test_timezone_preserved() {
        let content = load_fixture("with_timezone");
        let cal = parse_ics(&content).unwrap();
        let output = cal.to_ics_string();
        assert_eq!(cal.vtimezones.len(), 1);
        assert!(output.contains("BEGIN:VTIMEZONE"));
        assert!(output.contains("TZID:America/New_York"));
        assert!(output.contains("END:VTIMEZONE"));
    }

    #[test]
    fn test_x_properties_stripped() {
        let content = load_fixture("x_properties");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 1);
        let output = cal.to_ics_string();
        assert!(!output.contains("X-MY-CUSTOM-PROP"));
        assert!(!output.contains("X-GOOGLE-CONFERENCE"));
        assert!(!output.contains("X-APPLE-STRUCTURED-LOCATION"));
        assert!(!output.contains("should be stripped"));
    }

    #[test]
    fn test_multiple_events() {
        let content = load_fixture("multiple_events");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 3);
        let uids: Vec<&str> = cal
            .events
            .iter()
            .map(|e| {
                if e.uid.contains("multi-1") {
                    "multi-1"
                } else if e.uid.contains("multi-2") {
                    "multi-2"
                } else {
                    "multi-3"
                }
            })
            .collect();
        assert!(uids.contains(&"multi-1"));
        assert!(uids.contains(&"multi-2"));
        assert!(uids.contains(&"multi-3"));
        let output = cal.to_ics_string();
        // All three events should be in output with SUMMARY:Busy
        assert_eq!(output.matches("SUMMARY:Busy").count(), 3);
    }

    #[test]
    fn test_dedup_by_uid() {
        let mut merged = SanitizedCalendar::new();
        let e1 = SanitizedEvent {
            uid: "UID:dup".into(),
            dtstart: "DTSTART:20240601T090000Z".into(),
            dtend: Some("DTEND:20240601T100000Z".into()),
            duration: None,
            rrule: None,
            exdate: vec![],
            rdate: vec![],
            recurrence_id: None,
            transp: None,
            status: None,
        };
        let e2 = SanitizedEvent {
            uid: "UID:dup".into(),
            dtstart: "DTSTART:20240602T090000Z".into(),
            dtend: Some("DTEND:20240602T100000Z".into()),
            duration: None,
            rrule: None,
            exdate: vec![],
            rdate: vec![],
            recurrence_id: None,
            transp: None,
            status: None,
        };
        let other = SanitizedCalendar {
            vtimezones: vec![],
            events: vec![e1, e2],
        };
        merged.merge(other);
        assert_eq!(merged.events.len(), 1);
    }

    #[test]
    fn test_duration_event() {
        let content = load_fixture("duration");
        let cal = parse_ics(&content).unwrap();
        assert_eq!(cal.events.len(), 1);
        let event = &cal.events[0];
        assert!(event.duration.is_some());
        assert!(event.duration.as_ref().unwrap().contains("PT1H"));
    }

    #[test]
    fn test_malformed_ics() {
        let result = parse_ics("NOT VALID ICS CONTENT");
        assert!(result.is_err());
    }

    #[test]
    fn test_output_is_valid_ics() {
        let content = load_fixture("simple");
        let cal = parse_ics(&content).unwrap();
        let output = cal.to_ics_string();
        // Re-parse the output to verify it's valid ICS
        let reparsed = parse_ics(&output).unwrap();
        assert_eq!(reparsed.events.len(), 1);
        assert!(output.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(output.ends_with("END:VCALENDAR\r\n"));
    }

    #[test]
    fn test_empty_calendar_produces_valid_ics() {
        let cal = SanitizedCalendar::new();
        let output = cal.to_ics_string();
        assert!(output.starts_with("BEGIN:VCALENDAR"));
        assert!(output.contains("PRODID:-//Calendar Proxy//EN"));
        assert!(output.ends_with("END:VCALENDAR\r\n"));
        assert!(!output.contains("BEGIN:VEVENT"));
    }

    #[test]
    fn test_property_to_string() {
        let prop = IcalProperty {
            name: "DTSTART".into(),
            params: vec![("TZID".into(), vec!["America/New_York".into()])],
            value: "20240601T090000".into(),
        };
        let s = property_to_string(&prop);
        assert_eq!(s, "DTSTART;TZID=America/New_York:20240601T090000");
    }

    #[test]
    fn test_property_to_string_no_params() {
        let prop = IcalProperty {
            name: "UID".into(),
            params: vec![],
            value: "test-uid".into(),
        };
        let s = property_to_string(&prop);
        assert_eq!(s, "UID:test-uid");
    }

    #[test]
    fn test_extract_vtimezones_none() {
        let zones = extract_vtimezones("BEGIN:VCALENDAR\nEND:VCALENDAR");
        assert_eq!(zones.len(), 0);
    }

    #[test]
    fn test_extract_vtimezones_multiple() {
        let content = "BEGIN:VTIMEZONE\r\nTZID:America/New_York\r\nEND:VTIMEZONE\r\n\
                        BEGIN:VTIMEZONE\r\nTZID:Europe/London\r\nEND:VTIMEZONE";
        let zones = extract_vtimezones(content);
        assert_eq!(zones.len(), 2);
        assert!(zones[0].contains("America/New_York"));
        assert!(zones[1].contains("Europe/London"));
    }
}
