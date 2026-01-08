use reqwest;
use chrono::{NaiveDateTime, Utc};
use anyhow::Result;

const CALENDAR_URL: &'static str = "https://ics.ecal.com/ecal-sub/688cce50a0357c0008f39998/Liverpool%20FC.ics";

#[derive(Debug, Clone)]
pub struct Fixture {
    pub date: NaiveDateTime,
    pub opponent: String,
}

fn parse_ical(ical_data: &str) -> Result<Vec<Fixture>> {
    let reader = ical::IcalParser::new(ical_data.as_bytes());
    let mut fixtures = Vec::new();

    for calendar in reader {
        let cal = calendar?;
        for event in cal.events {
            // Extract summary (e.g., "Liverpool vs PSV" or "Internazionale vs Liverpool")
            let summary = event.properties.iter()
                .find(|p| p.name == "SUMMARY")
                .and_then(|p| p.value.clone());

            // Extract location to determine home/away
            let location = event.properties.iter()
                .find(|p| p.name == "LOCATION")
                .and_then(|p| p.value.clone());

            // Extract date
            let start = event.properties.iter()
                .find(|p| p.name == "DTSTART")
                .and_then(|p| p.value.clone());

            if let (Some(summary), Some(location), Some(date_str)) = (summary, location, start) {
                // Parse the date (iCal format: YYYYMMDDTHHMMSS or YYYYMMDDTHHMMSSZ)
                if let Ok(date) = NaiveDateTime::parse_from_str(&date_str.replace("Z", ""), "%Y%m%dT%H%M%S") {
                    // Determine opponent based on location
                    let is_home = location.contains("Anfield");

                    // Extract opponent from summary
                    // Remove emojis and clean up the summary
                    let clean_summary = summary
                        .chars()
                        .filter(|c| c.is_ascii() || c.is_whitespace())
                        .collect::<String>()
                        .trim()
                        .to_string();

                    if let Some(opponent) = extract_opponent(&clean_summary, is_home) {
                        fixtures.push(Fixture {
                            date,
                            opponent,
                        });
                    }
                }
            }
        }
    }

    // Sort fixtures by date
    fixtures.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(fixtures)
}

fn extract_opponent(summary: &str, is_home: bool) -> Option<String> {
    // Split by "vs"
    let parts: Vec<&str> = summary.split(" vs ").collect();

    if parts.len() == 2 {
        let opponent = if is_home {
            // Home game: "Liverpool vs Opponent"
            parts[1].trim()
        } else {
            // Away game: "Opponent vs Liverpool"
            parts[0].trim()
        };
        Some(opponent.to_string())
    } else {
        None
    }
}

pub async fn check_today_fixture() -> Result<Option<Fixture>> {
    let response = reqwest::get(CALENDAR_URL).await?;
    let ical_data = response.text().await?;

    // Parse fixtures
    let fixtures = parse_ical(&ical_data)?;

    let now = Utc::now().naive_utc();
    let today = now.date();

    let fixture = fixtures.iter()
        .find(|fixture| fixture.date.date() == today)
        .cloned();

    Ok(fixture)
}
