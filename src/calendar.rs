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
        Self { vtimezones: Vec::new(), events: Vec::new() }
    }

    pub fn merge(&mut self, other: SanitizedCalendar) {
        for tz in other.vtimezones {
            let tzid = extract_tzid(&tz);
            if !self.vtimezones.iter().any(|existing| extract_tzid(existing) == tzid) {
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
    IcalProperty { name, params, value }
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
        .map(|p| property_to_string(p))?;

    let dtstart = props
        .iter()
        .find(|p| p.name == "DTSTART")
        .map(|p| property_to_string(p))?;

    let dtend = props
        .iter()
        .find(|p| p.name == "DTEND")
        .map(|p| property_to_string(p));

    let duration = props
        .iter()
        .find(|p| p.name == "DURATION")
        .map(|p| property_to_string(p));

    let rrule = props
        .iter()
        .find(|p| p.name == "RRULE")
        .map(|p| property_to_string(p));

    let exdate: Vec<String> =
        props.iter().filter(|p| p.name == "EXDATE").map(property_to_string).collect();

    let rdate: Vec<String> =
        props.iter().filter(|p| p.name == "RDATE").map(property_to_string).collect();

    let recurrence_id = props
        .iter()
        .find(|p| p.name == "RECURRENCE-ID")
        .map(|p| property_to_string(p));

    let transp = props
        .iter()
        .find(|p| p.name == "TRANSP")
        .map(|p| property_to_string(p));

    let status = props
        .iter()
        .find(|p| p.name == "STATUS")
        .map(|p| property_to_string(p));

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
    loop {
        let begin = match upper[pos..].find("BEGIN:VTIMEZONE") {
            Some(idx) => pos + idx,
            None => break,
        };
        let end = match upper[begin..].find("END:VTIMEZONE") {
            Some(idx) => begin + idx + "END:VTIMEZONE".len(),
            None => break,
        };
        zones.push(content[begin..end].to_string());
        pos = end;
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
            let props: Vec<IcalProperty> =
                event.properties.iter().map(convert_property).collect();

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
