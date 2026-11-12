//! The `koma://` protocol: serves images straight to `<img>` tags.
//!
//! - `koma://page/{volumeId}/{index}?w={width}`: a page. With `w`, it is
//!   resized to that pixel width; without, the original bytes are returned.
//!   Served from the memory cache, which also preloads the pages around it.
//! - `koma://cover/{volumeId}`: cached cover (made on demand if missing)
//! - `koma://thumb/{volumeId}/{index}`: cached page thumbnail (made on demand)
//!
//! On Windows the webview sends these as `http://koma.localhost/page/…`.
//! Bytes never go through JSON or base64. Work runs on a background thread.

use std::fs;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::archive::mime_from_bytes;
use crate::state::AppState;

/// Widths outside this range are ignored and the original page is served.
const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 64..=8192;

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    // `koma://page/…` puts the kind in the host; the Windows form puts it in the path.
    let host = request.uri().host().unwrap_or("").to_string();
    let path = request.uri().path().to_string();
    let width = request.uri().query().and_then(width_from_query);
    tauri::async_runtime::spawn_blocking(move || {
        let mut segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
        if matches!(host.as_str(), "page" | "cover" | "thumb") {
            segments.insert(0, &host);
        }
        responder.respond(serve(&app, &segments, width));
    });
}

fn width_from_query(query: &str) -> Option<u32> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("w="))
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|width| WIDTH_RANGE.contains(width))
}

struct Image {
    bytes: Vec<u8>,
    mime: &'static str,
    /// Small, fixed images the webview may keep. Pages are large; Rust owns that cache.
    cacheable: bool,
}

fn serve<R: Runtime>(app: &AppHandle<R>, segments: &[&str], width: Option<u32>) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let index = |text: &str| text.parse::<usize>().map_err(|_| bad_request("page index must be a number"));
    let file = |path: std::path::PathBuf| {
        let bytes = fs::read(path).map_err(|err| not_found(err.to_string()))?;
        Ok(Image { mime: mime_from_bytes(&bytes), bytes, cacheable: true })
    };

    let result: Result<Image, Response<Vec<u8>>> = (|| match segments {
        ["page", volume_id, page] => {
            let page = index(page)?;
            let (archive, _) = state.open_volume(volume_id).map_err(not_found)?;
            let cached = state.pages.get(&archive, volume_id, page, width).map_err(not_found)?;
            state.pages.preload_around(&archive, volume_id, page, width);
            Ok(Image { bytes: cached.bytes.clone(), mime: cached.mime, cacheable: false })
        }
        ["cover", volume_id] => {
            let (archive, key) = state.open_volume(volume_id).map_err(not_found)?;
            file(state.thumbs.ensure_cover(&archive, &key).map_err(not_found)?)
        }
        ["thumb", volume_id, page] => {
            let page = index(page)?;
            let (archive, key) = state.open_volume(volume_id).map_err(not_found)?;
            file(state.thumbs.ensure_thumb(&archive, &key, page).map_err(not_found)?)
        }
        _ => Err(bad_request("unknown koma:// address")),
    })();

    match result {
        Ok(image) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, image.mime)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CACHE_CONTROL, if image.cacheable { "max-age=86400" } else { "no-store" })
            .body(image.bytes)
            .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "could not build response".into())),
        Err(response) => response,
    }
}

fn bad_request(message: &str) -> Response<Vec<u8>> {
    error(StatusCode::BAD_REQUEST, message.to_string())
}

fn not_found(message: String) -> Response<Vec<u8>> {
    error(StatusCode::NOT_FOUND, message)
}

fn error(status: StatusCode, message: String) -> Response<Vec<u8>> {
    let mut response = Response::new(message.into_bytes());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::width_from_query;

    #[test]
    fn reads_the_width_parameter() {
        assert_eq!(width_from_query("w=960"), Some(960));
        assert_eq!(width_from_query("x=1&w=1280"), Some(1280));
        assert_eq!(width_from_query("w=abc"), None);
        assert_eq!(width_from_query("w=10"), None);
        assert_eq!(width_from_query("w=999999"), None);
        assert_eq!(width_from_query("width=960"), None);
    }
}
