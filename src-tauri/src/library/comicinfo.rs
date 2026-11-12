//! Reads the few ComicInfo.xml fields KOMA uses. ComicInfo is a flat list of
//! simple elements, so the fields are picked out directly.

#[derive(Debug, Default, PartialEq)]
pub struct ComicInfo {
    pub series: Option<String>,
    /// `Number`, falling back to `Volume`.
    pub number: Option<f64>,
    /// `Manga`: `YesAndRightToLeft` → true, `No` → false, anything else unknown.
    pub right_to_left: Option<bool>,
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn element(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&close)?;
    let text = unescape(xml[start..end].trim());
    (!text.is_empty()).then_some(text)
}

pub fn parse(bytes: &[u8]) -> ComicInfo {
    let text = String::from_utf8_lossy(bytes);
    let xml = text.trim_start_matches('\u{feff}');
    let number = |tag| element(xml, tag).and_then(|value| value.parse::<f64>().ok());
    ComicInfo {
        series: element(xml, "Series"),
        number: number("Number").or_else(|| number("Volume")),
        right_to_left: match element(xml, "Manga").as_deref() {
            Some("YesAndRightToLeft") => Some(true),
            Some("No") => Some(false),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_series_number_and_direction() {
        let xml = "\u{feff}<?xml version=\"1.0\"?>\n<ComicInfo>\n  <Title>Rain</Title>\n  \
                   <Series>Paper Ronin &amp; Co</Series>\n  <Number>7</Number>\n  <Volume>2</Volume>\n  \
                   <Manga>YesAndRightToLeft</Manga>\n</ComicInfo>";
        assert_eq!(
            parse(xml.as_bytes()),
            ComicInfo {
                series: Some("Paper Ronin & Co".into()),
                number: Some(7.0),
                right_to_left: Some(true),
            }
        );
    }

    #[test]
    fn falls_back_to_volume_and_tolerates_missing_fields() {
        let info = parse(b"<ComicInfo><Series>X</Series><Volume>3</Volume><Manga>No</Manga></ComicInfo>");
        assert_eq!(info.number, Some(3.0));
        assert_eq!(info.right_to_left, Some(false));

        assert_eq!(parse(b"<ComicInfo><Series> </Series><Number>n/a</Number></ComicInfo>"), ComicInfo::default());
        assert_eq!(parse(b"not xml at all"), ComicInfo::default());
        assert_eq!(parse(&[0xFF, 0xFE, 0x00]), ComicInfo::default());
    }
}
