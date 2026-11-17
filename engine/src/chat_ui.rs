//! The follow-up chat panel shown under the findings on the Scan page. Kept out of
//! chat.rs (logic) and serve.rs (router), mirroring ai_ui.rs.

use crate::chat::ChatTurn;
use crate::lang::t;
use crate::util::esc;

/// Scoped styles for Markdown-rendered AI replies (`.ds-md`). Emitted once with
/// the chat panel (one panel per page). Keeps tables/code/headings legible inside
/// the chat bubble.
const MD_STYLE: &str = "<style>\
.ds-md{line-height:1.5}\
.ds-md>:first-child{margin-top:0}.ds-md>:last-child{margin-bottom:0}\
.ds-md h1,.ds-md h2,.ds-md h3,.ds-md h4{font-size:15px;margin:10px 0 6px;font-weight:700}\
.ds-md p{margin:6px 0}\
.ds-md ul,.ds-md ol{margin:6px 0;padding-left:20px}.ds-md li{margin:2px 0}\
.ds-md code{background:#eff1f3;border-radius:4px;padding:1px 5px;font-size:12.5px;\
font-family:ui-monospace,SFMono-Regular,Menlo,monospace}\
.ds-md pre{background:#eff1f3;border-radius:8px;padding:10px;overflow:auto}\
.ds-md pre code{background:none;padding:0}\
.ds-md table{border-collapse:collapse;margin:8px 0;font-size:13px;width:100%}\
.ds-md th,.ds-md td{border:1px solid #d0d7de;padding:5px 9px;text-align:left}\
.ds-md th{background:#f0f3f6}\
.ds-md blockquote{margin:6px 0;padding-left:12px;border-left:3px solid #d0d7de;color:#57606a}\
.ds-md a{color:#0969da}</style>";

/// The chat panel: the conversation so far + an input box. Only rendered when
/// findings exist and an AI backend is available (serve.rs gates it).
pub fn panel_html(history: &[ChatTurn], running: bool) -> String {
    let mut convo = String::new();
    for turn in history {
        if turn.role == "assistant" {
            // AI replies are Markdown (headings, tables, **bold**, `code`): render
            // them to safe HTML (raw HTML/images stripped in md.rs) inside a
            // `.ds-md` box styled by MD_STYLE below. Without this, `##`/`|---|`
            // showed up literally.
            convo.push_str(&format!(
                "<div style='margin:6px 0;text-align:left'>\
                 <div style='display:inline-block;max-width:92%;background:#f6f8fa;\
                 border:1px solid #d0d7de;border-radius:10px;padding:8px 12px;font-size:14px'>\
                 <div style='font-weight:600;color:#57606a;font-size:12px;margin-bottom:4px'>{ai}</div>\
                 <div class='ds-md'>{body}</div></div></div>",
                ai = t("AI", "AI"),
                body = crate::md::to_html(&turn.content),
            ));
        } else {
            // The user's own typed message — plain text, escaped, newlines kept.
            convo.push_str(&format!(
                "<div style='margin:6px 0;text-align:right'>\
                 <div style='display:inline-block;max-width:85%;text-align:left;background:#ddf4ff;\
                 border:1px solid #d0d7de;border-radius:10px;padding:8px 12px;font-size:14px;\
                 white-space:pre-wrap'>{you}: {body}</div></div>",
                you = t("You", "あなた"),
                body = esc(&turn.content),
            ));
        }
    }
    if running {
        convo.push_str(&format!(
            "<div style='margin:6px 0'><div style='display:inline-block;background:#f6f8fa;\
             border:1px solid #d0d7de;border-radius:10px;padding:8px 12px;font-size:14px'>{}</div></div>",
            t("AI is thinking…", "AI が考え中…"),
        ));
    }
    let placeholder = t(
        "Ask about these findings… (e.g. is this cache safe to delete?)",
        "検出結果について質問… (例: このキャッシュは消して大丈夫?)",
    );
    format!(
        "{MD_STYLE}\
         <h3 style='font-size:15px;margin:22px 0 6px'>💬 {h}</h3>\
         <div style='border:1px solid #d0d7de;border-radius:10px;padding:10px;background:#fff'>\
         {convo}\
         <form method='post' action='/chat' style='display:flex;gap:8px;margin-top:8px'>\
         <input type='text' name='msg' placeholder='{placeholder}' autocomplete='off' required \
         style='flex:1;font-size:14px;padding:8px'>\
         <button type='submit' style='background:#8250df;color:#fff;border:0;border-radius:8px;\
         padding:8px 16px;font-size:14px;cursor:pointer'>{send}</button></form></div>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        h = t("Ask the AI about these findings", "検出結果についてAIに聞く"),
        send = t("Send", "送信"),
        note = t(
            "The findings context is masked metadata only. Your typed messages are sent to the provider as-is. DiskSage never deletes from a chat — use the checkboxes above.",
            "検出結果のコンテキストはマスク済みメタデータのみ。入力した質問はそのままプロバイダに送信されます。チャットから削除は行いません（削除は上のチェックボックスから）。"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::panel_html;
    use crate::chat::ChatTurn;

    #[test]
    fn assistant_markdown_is_rendered_user_text_is_escaped() {
        let history = vec![
            ChatTurn {
                role: "user".into(),
                content: "is <b>this</b> safe?".into(),
            },
            ChatTurn {
                role: "assistant".into(),
                content: "## Yes\n\n| a | b |\n| - | - |\n| 1 | 2 |".into(),
            },
        ];
        let html = panel_html(&history, false);
        // AI reply rendered as Markdown inside a styled box.
        assert!(html.contains("class='ds-md'"), "ds-md wrapper: {html}");
        assert!(html.contains("<h2>Yes</h2>"), "heading rendered: {html}");
        assert!(html.contains("<table>"), "table rendered: {html}");
        // The user's own text stays escaped, never rendered as HTML.
        assert!(
            html.contains("&lt;b&gt;this&lt;/b&gt;"),
            "user escaped: {html}"
        );
    }
}
