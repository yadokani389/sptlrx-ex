use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PayloadError {
    InvalidJson,
    InvalidPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentLine {
    pub text: String,
    pub index: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricState {
    pub title: String,
    pub artists: Vec<String>,
    pub status: String,
    pub lines_count: usize,
    pub lyrics_panel_open: bool,
    pub current_line: Option<CurrentLine>,
    pub lines: Vec<String>,
    pub timestamp: String,
}

impl LyricState {
    pub fn waiting() -> Self {
        Self {
            title: String::new(),
            artists: Vec::new(),
            status: String::from("waiting"),
            lines_count: 0,
            lyrics_panel_open: false,
            current_line: None,
            lines: Vec::new(),
            timestamp: timestamp_now(),
        }
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, PayloadError> {
        let payload: Value =
            serde_json::from_slice(bytes).map_err(|_| PayloadError::InvalidJson)?;
        Self::from_json_value(payload)
    }

    fn from_json_value(payload: Value) -> Result<Self, PayloadError> {
        let object = payload.as_object().ok_or(PayloadError::InvalidPayload)?;

        let title = object
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let artists = object
            .get("artists")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let status = object
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let lines = sanitize_lines(object.get("lines"));

        let lines_count = object
            .get("linesCount")
            .and_then(|value| {
                if let Some(value) = value.as_u64() {
                    return Some(value as usize);
                }
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .map(|value| value as usize)
            })
            .unwrap_or(lines.len());

        let lyrics_panel_open = object
            .get("lyricsPanelOpen")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let current_line = sanitize_current_line(object.get("currentLine"));

        let timestamp = object
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(timestamp_now);

        Ok(Self {
            title,
            artists,
            status,
            lines_count,
            lyrics_panel_open,
            current_line,
            lines,
            timestamp,
        })
    }
}

fn sanitize_lines(value: Option<&Value>) -> Vec<String> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(600)
        .map(ToOwned::to_owned)
        .collect()
}

fn sanitize_current_line(value: Option<&Value>) -> Option<CurrentLine> {
    let object = value?.as_object()?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }

    let index = object.get("index").and_then(Value::as_i64).unwrap_or(-1);

    Some(CurrentLine { text, index })
}

fn timestamp_now() -> String {
    Timestamp::now().to_string()
}
