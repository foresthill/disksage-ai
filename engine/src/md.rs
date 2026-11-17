//! Minimal, safe Markdown → HTML for AI-generated text (the follow-up chat
//! replies). The model often answers with headings, **bold**, `code` and GFM
//! tables; without rendering, those show up as literal `##` / `|---|` in the
//! bubble (what the user reported).
//!
//! Safety: the reply comes from the user's configured AI provider and renders
//! inside the local webview, which can reach DiskSage's own POST endpoints
//! (`/delete` etc.). So we never pass author HTML through — raw HTML blocks,
//! inline HTML and images are dropped entirely, leaving only markup derived from
//! the Markdown itself. That blocks `<script>`, `onerror=` handlers and surprise
//! remote image loads. Output is meant to sit inside a `.ds-md` container (see
//! `chat_ui`) which styles the tables/code/headings.

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

/// Render Markdown to safe HTML (GFM tables / strikethrough / task-lists), with
/// all raw HTML and images stripped.
pub fn to_html(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    // Drop raw-HTML and image events; the alt-text Text events between an image's
    // Start/End are kept, so nothing readable is lost.
    let parser = Parser::new_ext(src, opts).filter(|ev| {
        !matches!(
            ev,
            Event::Html(_)
                | Event::InlineHtml(_)
                | Event::Start(Tag::HtmlBlock)
                | Event::End(TagEnd::HtmlBlock)
                | Event::Start(Tag::Image { .. })
                | Event::End(TagEnd::Image)
        )
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::to_html;

    #[test]
    fn renders_headings_bold_and_tables() {
        let html = to_html("## Title\n\n**bold** and `code`\n\n| a | b |\n| - | - |\n| 1 | 2 |\n");
        assert!(html.contains("<h2>"), "heading: {html}");
        assert!(html.contains("<strong>bold</strong>"), "bold: {html}");
        assert!(html.contains("<code>code</code>"), "code: {html}");
        assert!(
            html.contains("<table>") && html.contains("<td>1</td>"),
            "table: {html}"
        );
    }

    #[test]
    fn strips_raw_html_and_images() {
        let html = to_html("hi <script>alert(1)</script> there ![x](http://evil/i.png)");
        assert!(!html.contains("<script>"), "script stripped: {html}");
        assert!(!html.contains("<img"), "image stripped: {html}");
        assert!(
            html.contains("hi") && html.contains("there"),
            "text kept: {html}"
        );
    }
}
