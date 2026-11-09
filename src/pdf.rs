//! HTML → PDF in pure Rust: fulgur (Blitz layout: Stylo, Taffy, Parley; krilla writes the PDF).
//! No network, no scripts, no system fonts: text is set in bundled DejaVu whatever the document
//! asks for, so output is identical on every host. fulgur is young (0.x); expect gaps in CSS
//! support — the original file is always one click away.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use fulgur::{AssetBundle, Engine, Margin, PageSize};

/// Bitstream Vera / DejaVu license: `assets/fonts/LICENSE`.
const FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/DejaVuSans.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf"),
    include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
];

/// Without system fonts, generic families (`sans-serif`, `system-ui`, …) resolve to nothing and the
/// text silently disappears, so every element is pinned to the bundled families.
const FONT_CSS: &str = "* { font-family: 'DejaVu Sans' !important } \
                        code, pre, kbd, samp { font-family: 'DejaVu Sans Mono' !important }";

/// Largest document we lay out. Measured: 2 MB of paragraphs → 600 pages in 1 s, 280 MB peak.
pub const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
/// Deepest element nesting we accept. Layout recurses per level: real documents stay under 100,
/// past ~1000 the engine drops content, and near 5000 it overflows an 8 MB stack.
pub const MAX_DEPTH: usize = 512;
/// How long a request waits for a render; the thread finishes on its own after that.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// Layout recursion wants more stack than tokio's blocking threads carry. Virtual, committed lazily.
const STACK_BYTES: usize = 256 * 1024 * 1024;

pub fn render(html: &str, title: &str) -> Result<Vec<u8>> {
    let mut assets = AssetBundle::new();
    for font in FONTS {
        assets
            .add_font_bytes(font.to_vec())
            .map_err(|e| anyhow!("register font: {e}"))?;
    }
    assets.add_css(FONT_CSS);
    let engine = Engine::builder()
        .page_size(PageSize::A4)
        .margin(Margin::uniform_mm(15.0))
        .title(title)
        .assets(assets)
        .system_fonts(false)
        .build();
    engine.render(html).map_err(|e| anyhow!("render: {e}"))
}

/// `render` on its own big-stack thread, bounded by `TIMEOUT`.
pub async fn render_async(html: String, title: String) -> Result<Vec<u8>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("pdf-render".into())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let _ = tx.send(render(&html, &title));
        })
        .context("spawn pdf thread")?;
    match tokio::time::timeout(TIMEOUT, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(anyhow!("pdf thread died")),
        Err(_) => Err(anyhow!("pdf render timed out")),
    }
}

/// Element nesting depth from the raw markup, cheaply: counts open and close tags, ignores void and
/// self-closing elements, and skips `<script>` / `<style>` bodies, where `<` is not markup.
pub fn nesting_depth(html: &str) -> usize {
    const VOID: [&str; 14] = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    let bytes = html.as_bytes();
    let (mut depth, mut max, mut i) = (0usize, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'/') {
            depth = depth.saturating_sub(1);
            i += 2;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-') {
            end += 1;
        }
        if end == start {
            i += 1;
            continue;
        }
        let name = html[start..end].to_ascii_lowercase();
        if name == "script" || name == "style" {
            i = find_ci(bytes, format!("</{name}").as_bytes(), end).unwrap_or(bytes.len());
            continue;
        }
        let close = bytes[end..bytes.len().min(end + 1024)]
            .iter()
            .position(|&b| b == b'>')
            .map(|p| end + p);
        let self_closing = close.is_some_and(|c| c > end && bytes[c - 1] == b'/');
        if !VOID.contains(&name.as_str()) && !self_closing {
            depth += 1;
            max = max.max(depth);
        }
        i = close.map_or(end, |c| c + 1);
    }
    max
}

fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|p| from + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth() {
        assert_eq!(nesting_depth("<p>a<br>b<img src=x></p>"), 1);
        assert_eq!(nesting_depth("<div><div><span/></div></div>"), 2);
        assert_eq!(nesting_depth("<script>if(a<b){x<y}</script><p>x</p>"), 1);
        assert_eq!(nesting_depth("<STYLE>a<b</STYLE><ul><li>x</li></ul>"), 2);
        assert_eq!(nesting_depth(&"<div>".repeat(700)), 700);
    }

    #[test]
    fn renders_text_with_bundled_fonts() {
        let pdf = render(
            "<h1>Hello</h1><p>Body, <b>bold</b> and <code>mono</code>.</p>",
            "t",
        )
        .unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 10_000, "fonts embedded: {} bytes", pdf.len());
    }
}
