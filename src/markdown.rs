use pulldown_cmark::{Options, Parser, html};

/// Render Markdown into a standalone HTML document. Raw HTML inside the Markdown passes through
/// (owner-authored content; the content host sandboxes it anyway).
pub fn render_document(md: &str, title: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    let parser = Parser::new_ext(md, opts);
    let mut body = String::with_capacity(md.len() * 2);
    html::push_html(&mut body, parser);
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{}</title><style>{}</style></head><body><main class=\"md\">{}</main></body></html>",
        escape(title),
        CSS,
        body
    )
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

const CSS: &str = r#"
:root{color-scheme:light dark;--fg:#1b1a17;--bg:#fbfaf7;--muted:#6b6660;--line:#e5e1d8;--accent:#c2410c;--code:#f1ede4}
@media(prefers-color-scheme:dark){:root{--fg:#ebe7df;--bg:#15140f;--muted:#9a948a;--line:#2d2b24;--accent:#fb923c;--code:#201e18}}
html{background:var(--bg);color:var(--fg)}
body{margin:0;font:17px/1.6 Georgia,"Iowan Old Style","Times New Roman",serif}
.md{max-width:46rem;margin:0 auto;padding:3rem 1.25rem 6rem}
h1,h2,h3,h4{font-family:ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;line-height:1.2;letter-spacing:-.01em;margin:2.2em 0 .6em}
h1{font-size:2.2rem;margin-top:0}h2{font-size:1.5rem;border-bottom:1px solid var(--line);padding-bottom:.3em}h3{font-size:1.2rem}
a{color:var(--accent)}p,ul,ol{margin:0 0 1.1em}li{margin:.2em 0}
blockquote{margin:1.2em 0;padding:.2em 1.2em;border-left:3px solid var(--accent);color:var(--muted)}
code{font:.9em ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--code);padding:.1em .35em;border-radius:4px}
pre{background:var(--code);padding:1em 1.2em;border-radius:8px;overflow:auto}pre code{background:none;padding:0}
table{border-collapse:collapse;width:100%;margin:1.2em 0;font-size:.95em}th,td{border:1px solid var(--line);padding:.5em .7em;text-align:left}th{background:var(--code)}
img{max-width:100%;height:auto}hr{border:0;border-top:1px solid var(--line);margin:2.5em 0}
"#;
