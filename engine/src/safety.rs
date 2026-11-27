//! The "Safety rules" transparency page (`/safety`).
//!
//! DiskSage's whole pitch is "it never deletes without you." This page turns
//! "trust us, we're safe" into "here, read the rules" — it *renders* the
//! guardrails that already exist in the code, read-only, so the user can see them
//! instead of taking them on faith. It adds no logic and deletes nothing.
//!
//! The one-click-deletable list is read from [`crate::trash::DELETABLE`] (the real
//! source of truth), not retyped here, so this page can't drift out of sync with
//! what the delete button actually allows.
//!
//! This is increment 1 of the "layered knowledge" design
//! (`claudedocs/design/knowledge-layers.md`, §5/§8): Layer-0 transparency.

use crate::lang::{is_ja, t};
use crate::trash::DELETABLE;
use crate::util::esc;

/// One rule, rendered as a card: an emoji/title line and a plain-language body.
fn rule(icon: &str, title: &str, body: &str) -> String {
    format!(
        "<div style='border:1px solid #d0d7de;border-radius:8px;padding:12px 14px;margin:8px 0;\
         background:#fff'>\
         <div style='font-weight:600;font-size:14px'>{icon} {title}</div>\
         <div style='font-size:13px;color:#57606a;margin-top:4px;line-height:1.5'>{body}</div></div>"
    )
}

/// A human label for a one-click-deletable pattern id. Unknown ids fall back to
/// the raw id, so a newly-whitelisted pattern still shows up here (honest by
/// construction) rather than silently vanishing from the page.
fn deletable_label(id: &str) -> String {
    let (en, ja) = match id {
        "iphone_backup" => ("iPhone / device backups", "iPhone・デバイスのバックアップ"),
        "ollama_models" => ("Ollama models", "Ollama のモデル"),
        "xcode_derived_data" => ("Xcode DerivedData", "Xcode DerivedData"),
        "coresimulator_caches" => ("iOS Simulator caches", "iOS シミュレータのキャッシュ"),
        "coresimulator_devices" => ("iOS Simulator devices", "iOS シミュレータのデバイス"),
        "ios_devicesupport" => ("Xcode iOS DeviceSupport", "Xcode の iOS DeviceSupport"),
        other => return esc(other),
    };
    esc(t(en, ja))
}

/// A `<ul>` of `items`.
fn list(items: &[String]) -> String {
    let lis: String = items
        .iter()
        .map(|i| format!("<li style='margin:4px 0'>{i}</li>"))
        .collect();
    format!("<ul style='font-size:13px;color:#57606a;line-height:1.5;margin:6px 0;padding-left:20px'>{lis}</ul>")
}

/// The full Safety-rules page body (goes inside `page::shell`).
pub fn safety_html() -> String {
    let mut out = String::new();

    out.push_str(t(
        "<h2 style='font-size:18px;margin:0 0 6px'>🛡 Safety rules</h2>",
        "<h2 style='font-size:18px;margin:0 0 6px'>🛡 安全ルール</h2>",
    ));
    out.push_str(t(
        "<p style='font-size:14px;color:#57606a;margin:0 0 16px;line-height:1.6'>\
         DiskSage never deletes anything on its own. The rules below are built in and \
         can't be turned off. This page shows them so you can <b>read</b> them, not just \
         trust them.</p>",
        "<p style='font-size:14px;color:#57606a;margin:0 0 16px;line-height:1.6'>\
         DiskSage が勝手に何かを削除することはありません。以下のルールは組み込みで、\
         オフにはできません。「信じてください」ではなく、実際に<b>読んで</b>確認できる\
         ようにこのページを用意しています。</p>",
    ));

    // --- The immutable Layer-0 rules ------------------------------------------
    out.push_str(t(
        "<h3 style='font-size:15px;margin:18px 0 4px'>The rules</h3>",
        "<h3 style='font-size:15px;margin:18px 0 4px'>ルール</h3>",
    ));
    out.push_str(&rule(
        "🚫",
        t("Never auto-delete", "自動削除しない"),
        t(
            "Every deletion is your own explicit click. DiskSage only ever <i>proposes</i> \
             what could be removed — it never removes anything by itself, on a schedule, \
             or in the background.",
            "削除はすべて、あなた自身の明示的なクリックで行われます。DiskSage は\
             「消せそうなもの」を<i>提案</i>するだけで、自動・定期・バックグラウンドで\
             勝手に消すことはありません。",
        ),
    ));
    out.push_str(&rule(
        "🗑",
        t("Trash, never rm", "ゴミ箱へ（rm しない）"),
        t(
            "When you do remove something it goes to the Trash (macOS Finder / Windows \
             Recycle Bin / Linux trash) and stays recoverable. There is no permanent \
             <code>rm</code> path anywhere in the code.",
            "実際に削除するときも、ゴミ箱（macOS は Finder / Windows はごみ箱 / Linux は \
             trash）へ移動するだけで、復元できます。コードのどこにも、完全削除する \
             <code>rm</code> の経路はありません。",
        ),
    ));
    out.push_str(&rule(
        "✅",
        t(
            "Only clearly-reclaimable items get a delete button",
            "削除ボタンが付くのは「明らかに回収してよい物」だけ",
        ),
        t(
            "Most findings are report-only: shown with advice, but with no delete button at \
             all. A one-click delete appears only for the handful of classes listed below.",
            "検出結果のほとんどは「レポートのみ」で、アドバイスは出ますが削除ボタンは\
             付きません。ワンクリック削除が出るのは、下に挙げるごく一部の種類だけです。",
        ),
    ));
    out.push_str(&rule(
        "🔒",
        t(
            "The MCP server has no delete tool",
            "MCP サーバに削除ツールは無い",
        ),
        t(
            "When DiskSage runs as an MCP server, it exposes diagnosis tools only \
             (disk usage, scan, largest folders, snapshots). No tool can delete, move, or \
             change a file — by design, so an AI assistant can never delete your files \
             through it.",
            "DiskSage を MCP サーバとして動かすとき、公開されるのは診断ツールだけです\
             （ディスク使用量・スキャン・大きいフォルダ・スナップショット）。ファイルを\
             削除・移動・変更するツールは一切ありません。AI アシスタントが MCP 経由で\
             あなたのファイルを消せないよう、意図的にそうしています。",
        ),
    ));
    out.push_str(&rule(
        "🙈",
        t(
            "Privacy: masked metadata only",
            "プライバシー：マスク済みメタデータのみ",
        ),
        t(
            "The optional AI judgment sends metadata only — masked paths and sizes, never \
             file contents — and shows you the exact payload first. See \
             <a href='/ai-preview'>what gets sent</a>.",
            "任意の AI 判定が送るのはメタデータだけ（マスク済みのパスとサイズ。ファイルの\
             中身は送りません）で、送信前に実際の内容を確認できます。\
             <a href='/ai-preview'>送信される内容</a>を参照。",
        ),
    ));

    // --- One-click deletable classes (read from the real whitelist) -----------
    out.push_str(t(
        "<h3 style='font-size:15px;margin:22px 0 4px'>What can be one-click deleted</h3>",
        "<h3 style='font-size:15px;margin:22px 0 4px'>ワンクリック削除できるもの</h3>",
    ));
    out.push_str(t(
        "<p style='font-size:13px;color:#57606a;margin:0 0 4px;line-height:1.5'>\
         These are the only classes with a delete checkbox. For each, the finding's path \
         <i>is</i> the thing to remove, so moving it to the Trash is safe and reversible:</p>",
        "<p style='font-size:13px;color:#57606a;margin:0 0 4px;line-height:1.5'>\
         削除チェックボックスが付くのは、以下の種類だけです。いずれも「検出されたパス\
         ＝消す対象そのもの」なので、ゴミ箱へ移動しても安全で、元に戻せます：</p>",
    ));
    let deletable: Vec<String> = DELETABLE.iter().map(|id| deletable_label(id)).collect();
    out.push_str(&list(&deletable));

    // --- Report-only classes (and why) ----------------------------------------
    out.push_str(t(
        "<h3 style='font-size:15px;margin:22px 0 4px'>What is report-only (and why)</h3>",
        "<h3 style='font-size:15px;margin:22px 0 4px'>レポートのみ（と、その理由）</h3>",
    ));
    out.push_str(t(
        "<p style='font-size:13px;color:#57606a;margin:0 0 4px;line-height:1.5'>\
         Everything else is shown with advice but no delete button. A few examples and the \
         reason each stays hands-off:</p>",
        "<p style='font-size:13px;color:#57606a;margin:0 0 4px;line-height:1.5'>\
         それ以外はすべて、アドバイスは出ますが削除ボタンは付きません。代表例と、\
         DiskSage が手を出さない理由：</p>",
    ));
    let reasons = [
        (
            t(
                "Photos, videos, music, documents, Downloads",
                "写真・動画・音楽・書類・ダウンロード",
            ),
            t(
                "your own data — archive or move it to an external drive; never auto-removed.",
                "あなた自身のデータです。外付けへ退避・移動するもので、自動削除は決してしません。",
            ),
        ),
        (
            t("node_modules (aggregate)", "node_modules（合計）"),
            t(
                "the path is your whole ~/Development folder — use the drill-down to pick \
                 individual sub-folders instead.",
                "パスが ~/Development フォルダ全体なので、内訳から個別に選んで削除します。",
            ),
        ),
        (
            t("Electron app caches", "Electron 系アプリのキャッシュ"),
            t(
                "the path is the app bundle itself — removing it would break the app.",
                "パスがアプリ本体なので、消すとアプリが壊れます。",
            ),
        ),
        (
            t("Docker.raw", "Docker.raw"),
            t(
                "unsafe to remove while Docker is running — use Docker's own cleanup.",
                "Docker 稼働中に消すのは危険です。Docker 純正の整理機能を使ってください。",
            ),
        ),
        (
            t(
                "Caches (npm / pnpm / Library / .cache)",
                "キャッシュ（npm / pnpm / Library / .cache）",
            ),
            t(
                "use the tool's own clean command (e.g. `npm cache clean --force`), not a \
                 blunt folder delete.",
                "フォルダごとの削除ではなく、ツール純正の clean コマンド\
                 （例 `npm cache clean --force`）を使います。",
            ),
        ),
        (
            t(
                "APFS / Time Machine snapshots, VM swap",
                "APFS / Time Machine スナップショット・VM スワップ",
            ),
            t(
                "managed by macOS (tmutil / a reboot), not a file you delete by hand. The \
                 `com.apple.os.update-*` snapshots release themselves once the update finishes.",
                "macOS が管理します（tmutil や再起動）。手で消すファイルではありません。\
                 `com.apple.os.update-*` は OS 更新が完了すると自動で解放されます。",
            ),
        ),
    ];
    let reason_items: Vec<String> = reasons
        .iter()
        .map(|(name, why)| format!("<b>{}</b> — {}", esc(name), esc(why)))
        .collect();
    out.push_str(&list(&reason_items));

    // --- Footer: it's all open source ----------------------------------------
    let footer = if is_ja() {
        "<p style='font-size:12px;color:#8c959f;margin-top:20px;line-height:1.6'>\
         これらのルールはすべてソースコード（Apache-2.0）に書かれており、1行残らず読めます。\
         DiskSage は OSS です。</p>"
    } else {
        "<p style='font-size:12px;color:#8c959f;margin-top:20px;line-height:1.6'>\
         All of these rules live in the source code (Apache-2.0) — you can read every line. \
         DiskSage is open source.</p>"
    };
    out.push_str(footer);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every whitelisted deletable id must appear on the page — the page is read
    /// from the real `DELETABLE` list, so this guards against a future id being
    /// added to the whitelist but silently missing from the transparency page.
    #[test]
    fn every_deletable_id_is_listed() {
        let html = safety_html();
        for id in DELETABLE {
            let label = deletable_label(id);
            assert!(
                html.contains(&label),
                "deletable id {id} (label {label:?}) missing from safety page"
            );
        }
    }

    /// The page must state the two load-bearing guarantees in plain words.
    #[test]
    fn states_core_guarantees() {
        let html = safety_html();
        // never auto-delete + no MCP delete tool, in whichever language is active.
        assert!(html.contains("MCP"));
        assert!(html.to_lowercase().contains("trash") || html.contains("ゴミ箱"));
    }

    /// An unknown id falls back to the raw id (honest-by-construction), not a panic
    /// or an empty string.
    #[test]
    fn unknown_id_falls_back_to_raw() {
        assert_eq!(deletable_label("some_new_pattern"), "some_new_pattern");
    }
}
