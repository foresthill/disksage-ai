//! The served Settings screen: report-language selection, read from and written
//! to the same `$DISKSAGE_HOME/config` that the bash CLI and bash serve use, so
//! the setting is consistent across all three.

use std::fs;
use std::path::PathBuf;

use crate::lang::t;
use crate::util::{esc, home_dir, percent_decode};

/// $DISKSAGE_HOME/config (default ~/.disksage/config).
fn config_path() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("config")
}

/// Value of a `key=` line in the config ("" if unset), from the shared
/// `$DISKSAGE_HOME/config` the bash CLI, bash serve and engine all read.
pub fn read_key(key: &str) -> String {
    let prefix = format!("{key}=");
    if let Ok(txt) = fs::read_to_string(config_path()) {
        for line in txt.lines() {
            if let Some(v) = line.strip_prefix(&prefix) {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

/// Replace (or, for an empty value, remove) the `key=` line, preserving others.
pub fn write_key(key: &str, val: &str) {
    let path = config_path();
    let prefix = format!("{key}=");
    let mut lines: Vec<String> = Vec::new();
    if let Ok(txt) = fs::read_to_string(&path) {
        for line in txt.lines() {
            if !line.starts_with(&prefix) {
                lines.push(line.to_string());
            }
        }
    }
    if !val.is_empty() {
        lines.push(format!("{key}={val}"));
    }
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    let _ = fs::write(&path, out);
}

/// Current `lang=` value ("" if unset → auto-detect).
pub fn read_lang() -> String {
    read_key("lang")
}

/// Replace (or, for an empty value, remove) the `lang=` line, preserving others.
pub fn write_lang(val: &str) {
    write_key("lang", val);
}

/// Value of a key in an `application/x-www-form-urlencoded` body.
pub fn form_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key {
            Some(v)
        } else {
            None
        }
    })
}

/// Apply a POST from the Settings screen. Two independent forms post here: the
/// language radios, and the AI-key form. The API key is written to the OS
/// keychain only — never to the config file.
pub fn handle_post(body: &str) {
    // Language form (its radio is always present when that form is submitted).
    if let Some(val) = form_value(body, "lang") {
        if matches!(val, "" | "ja" | "en") {
            write_lang(val);
            crate::lang::refresh();
        }
        return;
    }
    // Update-check form (a checkbox: present only when ticked; default on, so
    // we store `off` only when it comes back unchecked).
    if form_value(body, "update_action") == Some("set") {
        let on = form_value(body, "update_check").is_some();
        write_key("update_check", if on { "" } else { "off" });
        return;
    }
    // AI-provider selector (which backend to USE).
    if form_value(body, "ai_provider_action") == Some("set") {
        let p = form_value(body, "ai_provider").unwrap_or("");
        if matches!(p, "" | "anthropic" | "openrouter" | "ollama" | "claude-cli") {
            write_key("ai_provider", p);
        }
        return;
    }
    // Ollama connection form: base URL (port-forward friendly) + chosen model.
    if form_value(body, "ai_action") == Some("ollama_save") {
        if let Some(u) = form_value(body, "ollama_url") {
            let u = percent_decode(u);
            write_key("ollama_url", u.trim().trim_end_matches('/'));
        }
        if let Some(m) = form_value(body, "ollama_model") {
            let m = percent_decode(m);
            write_key("ollama_model", m.trim());
        }
        return;
    }
    // AI-key form.
    match form_value(body, "ai_action") {
        Some("save") => {
            let account = if form_value(body, "key_provider") == Some("openrouter") {
                crate::secret::OPENROUTER
            } else {
                crate::secret::ANTHROPIC
            };
            if let Some(raw) = form_value(body, "ai_key") {
                let key = percent_decode(raw);
                let key = key.trim();
                if !key.is_empty() {
                    let _ = crate::secret::set(account, key);
                }
            }
        }
        Some("clear") => {
            let _ = crate::secret::delete(crate::secret::ANTHROPIC);
            let _ = crate::secret::delete(crate::secret::OPENROUTER);
        }
        _ => {}
    }
}

/// The AI (BYOK) key section: a masked field to store a key in the OS keychain,
/// the current stored status, and a Clear button. The key value is never shown.
fn ai_section() -> String {
    let has_a = crate::secret::has(crate::secret::ANTHROPIC);
    let has_o = crate::secret::has(crate::secret::OPENROUTER);
    let status = if has_a || has_o {
        let which = if has_a && has_o {
            "Anthropic + OpenRouter"
        } else if has_a {
            "Anthropic"
        } else {
            "OpenRouter"
        };
        format!(
            "{} {which}",
            t(
                "🔑 Key stored in your OS keychain:",
                "🔑 OS キーチェーンに保存済み:"
            )
        )
    } else {
        t(
            "No BYOK key stored. (AI also works via Claude Code with no key.)",
            "BYOK キーは未保存。（Claude Code があればキー無しでも AI 判定できます。）",
        )
        .to_string()
    };
    let clear = if has_a || has_o {
        format!(
            "<form method='post' action='/settings' style='margin-top:8px'>\
             <input type='hidden' name='ai_action' value='clear'>\
             <button type='submit' style='background:#6e7781;color:#fff;border:0;border-radius:8px;\
             padding:7px 14px;font-size:13px;cursor:pointer'>{}</button></form>",
            t("Clear stored keys", "保存キーを削除")
        )
    } else {
        String::new()
    };
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <div style='color:#57606a;font-size:13px;margin-bottom:8px'>{status}</div>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='ai_action' value='save'>\
         <select name='key_provider' style='font-size:14px;padding:6px'>\
         <option value='anthropic'>Anthropic</option>\
         <option value='openrouter'>OpenRouter</option></select> \
         <input type='password' name='ai_key' placeholder='sk-…' autocomplete='off' \
         style='font-size:14px;padding:6px;width:240px'> \
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:8px 16px;font-size:14px;cursor:pointer'>{save}</button></form>{clear}\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        h = t("AI (BYOK) key", "AI（BYOK）キー"),
        save = t("Save key", "キーを保存"),
        note = t(
            "Stored in your OS keychain — never written to disk in plaintext. An env var takes precedence if set.",
            "OS キーチェーンに保存（平文でディスクに書きません）。環境変数があればそちらが優先。"
        ),
    )
}

/// The AI-provider selector: which backend DiskSage uses for a judgment. Auto
/// picks a BYOK key if present, else Claude Code. Explicit choices override it.
fn provider_section() -> String {
    let cur = read_key("ai_provider");
    let opt = |val: &str, label: &str| {
        let sel = if val == cur { "selected" } else { "" };
        format!("<option value='{val}' {sel}>{label}</option>")
    };
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='ai_provider_action' value='set'>\
         <select name='ai_provider' style='font-size:14px;padding:6px'>{}{}{}{}{}</select> \
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:7px 14px;font-size:13px;cursor:pointer'>{save}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        opt("", t("Auto", "自動")),
        opt("anthropic", "Anthropic (BYOK)"),
        opt("openrouter", "OpenRouter (BYOK)"),
        opt("ollama", t("Ollama (local)", "Ollama（ローカル）")),
        opt("claude-cli", "Claude Code"),
        h = t("AI provider", "AI プロバイダ"),
        save = t("Save", "保存"),
        note = t(
            "Which backend to use for the AI judgment. For Ollama, set the URL and model below.",
            "AI判定に使うバックエンド。Ollama を選んだら下で URL とモデルを設定してください。"
        ),
    )
}

/// The Ollama connection section: a configurable base URL (port-forward friendly)
/// and a model picker. On render it probes `GET /api/tags` (fast; a localhost
/// refusal is instant) to show a live connection status and populate the model
/// list — so "Save & test" doubles as the connection test.
fn ollama_section() -> String {
    let cur_url = read_key("ollama_url");
    let cur_model = read_key("ollama_model");
    let url_val = if cur_url.is_empty() {
        String::new()
    } else {
        esc(&cur_url)
    };
    let probe = crate::ollama::list_models();
    let (status, model_field) = match &probe {
        Ok(models) => {
            let status = format!(
                "<div style='color:#1a7f37;font-size:13px;margin-bottom:8px'>✅ {} · {} {}</div>",
                t("Connected", "接続OK"),
                models.len(),
                t("models", "モデル"),
            );
            // Dropdown of installed models. Keep the saved one selected even if it
            // isn't in the live list (server may be temporarily different).
            let mut opts = format!(
                "<option value=''>{}</option>",
                t("— choose a model —", "— モデルを選択 —")
            );
            let mut saw_cur = cur_model.is_empty();
            for m in models {
                let sel = if *m == cur_model {
                    saw_cur = true;
                    "selected"
                } else {
                    ""
                };
                opts.push_str(&format!("<option value='{0}' {sel}>{0}</option>", esc(m)));
            }
            if !saw_cur {
                opts.push_str(&format!(
                    "<option value='{0}' selected>{0}</option>",
                    esc(&cur_model)
                ));
            }
            (
                status,
                format!(
                    "<select name='ollama_model' style='font-size:14px;padding:6px;min-width:220px'>{opts}</select>"
                ),
            )
        }
        Err(e) => {
            let status = format!(
                "<div style='color:#cf222e;font-size:13px;margin-bottom:8px'>⚠️ {}: {}</div>",
                t("Not reachable", "接続できません"),
                esc(e),
            );
            // Fallback: a free-text model field so the user can still set one.
            let field = format!(
                "<input type='text' name='ollama_model' value='{}' placeholder='llama3.2' \
                 style='font-size:14px;padding:6px;width:220px'>",
                esc(&cur_model)
            );
            (status, field)
        }
    };
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>{status}\
         <form method='post' action='/settings'>\
         <input type='hidden' name='ai_action' value='ollama_save'>\
         <div style='margin-bottom:8px'><label style='font-size:13px;color:#57606a'>{url_l}</label><br>\
         <input type='text' name='ollama_url' value='{url_val}' placeholder='{ph}' \
         style='font-size:14px;padding:6px;width:300px'></div>\
         <div style='margin-bottom:8px'><label style='font-size:13px;color:#57606a'>{model_l}</label><br>\
         {model_field}</div>\
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:8px 16px;font-size:14px;cursor:pointer'>{save}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        h = t("Ollama (local AI)", "Ollama（ローカルAI）"),
        url_l = t("Server URL", "サーバ URL"),
        ph = crate::ollama::DEFAULT_URL,
        model_l = t("Model", "モデル"),
        save = t("💾 Save & test", "💾 保存してテスト"),
        note = t(
            "No API key — Ollama runs on your machine. Default port is 11434; change the URL if you port-forward. Saving re-tests the connection and refreshes the model list. Only masked metadata is sent, never file contents.",
            "API キー不要 — Ollama は端末上で動きます。既定ポートは 11434。ポートフォワーディングで変わる場合は URL を変更。保存で接続を再テストしモデル一覧を更新します。送信はマスク済みメタデータのみ（ファイル内容は送りません）。"
        ),
    )
}

/// The update-check section: a single checkbox (default on). Explains that it
/// only reads the public GitHub Releases page and never auto-downloads.
fn update_section() -> String {
    let on = read_key("update_check") != "off";
    let checked = if on { "checked" } else { "" };
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='update_action' value='set'>\
         <label style='font-size:14px'>\
         <input type='checkbox' name='update_check' {checked}> {lbl}</label> \
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:7px 14px;font-size:13px;cursor:pointer;margin-left:8px'>{save}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        h = t("Update check", "アップデート確認"),
        lbl = t(
            "Check GitHub for a newer version on launch",
            "起動時に新しいバージョンを GitHub で確認する"
        ),
        save = t("Save", "保存"),
        note = t(
            "Reads only the public Releases page — nothing about you or your files is sent, \
             and nothing is downloaded or installed automatically. It just shows a link.",
            "公開の Releases ページを見るだけです。あなたやファイルの情報は一切送信せず、\
             自動ダウンロード／インストールもしません。リンクを表示するだけです。"
        ),
    )
}

/// The `/settings` page body (sits inside the served shell).
pub fn settings_html(saved: bool) -> String {
    let cur = read_lang();
    let data_dir = config_path()
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let opt = |val: &str, label: &str| {
        let checked = if val == cur { "checked" } else { "" };
        format!(
            "<label style='display:block;padding:7px 0;font-size:14px'>\
             <input type='radio' name='lang' value='{val}' {checked}> {label}</label>"
        )
    };
    let saved_banner = if saved {
        t(
            "<div style='background:#dafbe1;border:1px solid #4ac26b;border-radius:8px;\
             padding:10px 14px;margin-bottom:14px;font-size:14px'>✅ Saved</div>",
            "<div style='background:#dafbe1;border:1px solid #4ac26b;border-radius:8px;\
             padding:10px 14px;margin-bottom:14px;font-size:14px'>✅ 保存しました</div>",
        )
    } else {
        ""
    };
    format!(
        "<h2 style='font-size:18px;margin:2px 0 14px'>{title}</h2>{saved_banner}\
         <form method='post' action='/settings'>\
         <h3 style='font-size:15px;margin:0 0 6px'>{lang_h}</h3>{}{}{}\
         <div style='color:#57606a;font-size:12px;margin:6px 0 14px'>{note}</div>\
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:9px 18px;font-size:14px;cursor:pointer'>{save}</button></form>\
         {provider}{ai}{ollama}{update}\
         <h3 style='font-size:15px;margin:24px 0 6px'>{about}</h3>\
         <div style='color:#57606a;font-size:13px;line-height:1.7'>{data_l} <code>{}</code><br>{never}</div>",
        opt("", t("Auto (system)", "自動（システム）")),
        opt("ja", "日本語"),
        opt("en", "English"),
        esc(&data_dir),
        provider = provider_section(),
        ai = ai_section(),
        ollama = ollama_section(),
        update = update_section(),
        title = t("⚙️ Settings", "⚙️ 設定"),
        lang_h = t("Report language", "レポートの言語"),
        note = t(
            "Applies immediately to the whole UI — no restart needed.",
            "UI 全体にすぐ反映されます（再起動不要）。"
        ),
        save = t("Save", "保存"),
        about = t("About", "情報"),
        data_l = t("Data:", "データ:"),
        never = t(
            "DiskSage never deletes anything automatically.",
            "DiskSage は何も自動削除しません。"
        ),
    )
}
