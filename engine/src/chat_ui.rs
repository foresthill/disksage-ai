//! The follow-up chat panel shown under the findings on the Scan page. Kept out of
//! chat.rs (logic) and serve.rs (router), mirroring ai_ui.rs.

use crate::chat::ChatTurn;
use crate::lang::t;
use crate::util::esc;

/// The chat panel: the conversation so far + an input box. Only rendered when
/// findings exist and an AI backend is available (serve.rs gates it).
pub fn panel_html(history: &[ChatTurn], running: bool) -> String {
    let mut convo = String::new();
    for turn in history {
        let (who, bg, align) = if turn.role == "assistant" {
            (t("AI", "AI"), "#f6f8fa", "left")
        } else {
            (t("You", "あなた"), "#ddf4ff", "right")
        };
        convo.push_str(&format!(
            "<div style='margin:6px 0;text-align:{align}'>\
             <div style='display:inline-block;max-width:85%;text-align:left;background:{bg};\
             border:1px solid #d0d7de;border-radius:10px;padding:8px 12px;font-size:14px;\
             white-space:pre-wrap'>{label}: {body}</div></div>",
            label = who,
            body = esc(&turn.content),
        ));
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
        "<h3 style='font-size:15px;margin:22px 0 6px'>💬 {h}</h3>\
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
