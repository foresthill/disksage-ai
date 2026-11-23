//! The served Settings screen: report-language selection, read from and written
//! to the same `$DISKSAGE_HOME/config` that the bash CLI and bash serve use, so
//! the setting is consistent across all three.

use std::fs;
use std::path::PathBuf;

use crate::lang::{is_ja, t};
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
/// Returns `true` when the resolved report language actually changed, so the
/// caller can regenerate the findings (their text is baked at scan time).
pub fn handle_post(body: &str) -> bool {
    // Language form (its radio is always present when that form is submitted).
    if let Some(val) = form_value(body, "lang") {
        if matches!(val, "" | "ja" | "en") {
            let was_ja = crate::lang::is_ja();
            write_lang(val);
            crate::lang::refresh();
            return crate::lang::is_ja() != was_ja;
        }
        return false;
    }
    // Update-check form (a checkbox: present only when ticked; default on, so
    // we store `off` only when it comes back unchecked).
    if form_value(body, "update_action") == Some("set") {
        let on = form_value(body, "update_check").is_some();
        write_key("update_check", if on { "" } else { "off" });
        return false;
    }
    // Trend sampler form: interval (how often = resolution) and retention (how
    // far back to keep). They're independent, so changing one never silently
    // changes the other. Both are whitelisted.
    if form_value(body, "trend_action") == Some("set") {
        if let Some(v) = form_value(body, "trend_interval") {
            if matches!(v, "off" | "15" | "30" | "60") {
                write_key("trend_interval", v);
            }
        }
        if let Some(v) = form_value(body, "trend_retention") {
            if matches!(v, "7" | "21" | "30" | "365" | "forever") {
                write_key("trend_retention", v);
            }
        }
        return false;
    }
    // AI audit-log toggle (a checkbox: present only when ticked). Default off, so
    // we store `on` only when it comes back checked.
    if form_value(body, "audit_action") == Some("set") {
        let on = form_value(body, "ai_audit_log").is_some();
        write_key("ai_audit_log", if on { "on" } else { "" });
        return false;
    }
    // AI-provider selector (which backend to USE).
    if form_value(body, "ai_provider_action") == Some("set") {
        let p = form_value(body, "ai_provider").unwrap_or("");
        if matches!(p, "" | "anthropic" | "openrouter" | "ollama" | "claude-cli") {
            write_key("ai_provider", p);
        }
        return false;
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
        return false;
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
    false
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

/// Build the model `<select>` options: a blank choice + each installed model,
/// keeping the saved model selected (and present even if the live list lacks it).
fn model_options(models: &[String], cur: &str) -> String {
    let mut opts = format!(
        "<option value=''>{}</option>",
        t("— choose a model —", "— モデルを選択 —")
    );
    let mut saw_cur = cur.is_empty();
    for m in models {
        let sel = if m == cur {
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
            esc(cur)
        ));
    }
    opts
}

/// The Ollama connection section: a configurable base URL (port-forward friendly)
/// and a model picker. The **"Test & fetch models"** button probes the URL the
/// user is *typing* (via `GET /ollama-models` → live `/api/tags`) and fills the
/// dropdown — so you point at a server first, then pick from its real models,
/// without a save round-trip. The saved URL is also probed on load for immediate
/// status, and "Save" persists the choice.
fn ollama_section() -> String {
    let cur_url = read_key("ollama_url");
    let cur_model = read_key("ollama_model");
    let url_val = if cur_url.is_empty() {
        String::new()
    } else {
        esc(&cur_url)
    };
    // Load-time probe of the *saved* URL: immediate status + initial model list.
    let (status, options) = match crate::ollama::list_models() {
        Ok(models) => (
            format!(
                "<span style='color:#1a7f37'>🟢 {} · {} {}</span>",
                t("Connected", "接続OK"),
                models.len(),
                t("models", "モデル"),
            ),
            model_options(&models, &cur_model),
        ),
        Err(e) => (
            format!(
                "<span style='color:#cf222e'>🔴 {}: {}</span>",
                t("Not reachable", "接続できません"),
                esc(&e),
            ),
            // Keep only the saved model (if any) until the user tests a URL.
            model_options(&[], &cur_model),
        ),
    };
    // Small client script: fetch /api/tags for the TYPED url and rebuild the
    // dropdown. textContent everywhere (no markup injected from the error string).
    let js = r#"<script>
function dsStat(st,c,txt){var sp=document.createElement('span');sp.style.color=c;sp.textContent=txt;st.innerHTML='';st.appendChild(sp);}
async function dsOllamaTest(){
 var u=document.getElementById('om_url'),s=document.getElementById('om_sel'),st=document.getElementById('om_status');
 dsStat(st,'#57606a','__TESTING__');
 try{
  var r=await fetch('/ollama-models?url='+encodeURIComponent(u.value.trim()));
  var j=await r.json();
  if(j.ok){
   var cur=s.value;s.innerHTML='';
   var o0=document.createElement('option');o0.value='';o0.textContent='__CHOOSE__';s.appendChild(o0);
   j.models.forEach(function(m){var o=document.createElement('option');o.value=m;o.textContent=m;if(m===cur)o.selected=true;s.appendChild(o);});
   dsStat(st,'#1a7f37','🟢 '+j.models.length+' __MODELS__');
  }else{dsStat(st,'#cf222e','🔴 '+(j.error||'error'));}
 }catch(e){dsStat(st,'#cf222e','🔴 '+e);}
}
</script>"#
        .replace("__TESTING__", t("Testing…", "テスト中…"))
        .replace("__CHOOSE__", t("— choose a model —", "— モデルを選択 —"))
        .replace("__MODELS__", t("models", "モデル"));
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <div id='om_status' style='font-size:13px;margin-bottom:8px'>{status}</div>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='ai_action' value='ollama_save'>\
         <div style='margin-bottom:8px'><label style='font-size:13px;color:#57606a'>{url_l}</label><br>\
         <input type='text' id='om_url' name='ollama_url' value='{url_val}' placeholder='{ph}' \
         style='font-size:14px;padding:6px;width:300px'> \
         <button type='button' onclick='dsOllamaTest()' style='background:#2da44e;color:#fff;border:0;\
         border-radius:8px;padding:7px 14px;font-size:13px;cursor:pointer'>{test}</button></div>\
         <div style='margin-bottom:8px'><label style='font-size:13px;color:#57606a'>{model_l}</label><br>\
         <select id='om_sel' name='ollama_model' style='font-size:14px;padding:6px;min-width:220px'>{options}</select></div>\
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:8px 16px;font-size:14px;cursor:pointer'>{save}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>{js}",
        h = t("Ollama (local AI)", "Ollama（ローカルAI）"),
        url_l = t("Server URL", "サーバ URL"),
        ph = crate::ollama::DEFAULT_URL,
        model_l = t("Model", "モデル"),
        test = t("🔌 Test & fetch models", "🔌 接続テスト・モデル取得"),
        save = t("💾 Save", "💾 保存"),
        note = t(
            "No API key — Ollama runs on your machine. Default port is 11434; change the URL if you port-forward (e.g. an SSH tunnel), then click “Test & fetch models” to load its models. Save to keep the choice. Only masked metadata is sent, never file contents.",
            "API キー不要 — Ollama は端末上で動きます。既定ポートは 11434。ポートフォワーディング（SSH トンネル等）で変わる場合は URL を変更し「接続テスト・モデル取得」でモデルを読み込みます。「保存」で確定。送信はマスク済みメタデータのみ（ファイル内容は送りません）。"
        ),
    )
}

/// The AI audit-log section: a checkbox to save exactly what is sent to / returned
/// from the AI (request, response, masking table) under `~/.disksage/ai-logs/`,
/// plus a button to open that folder. Default off — the logs include real local
/// paths, so they stay on your machine and are opt-in.
fn audit_section() -> String {
    let on = read_key("ai_audit_log") == "on";
    let checked = if on { "checked" } else { "" };
    let dir = crate::audit::logs_dir().display().to_string();
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='audit_action' value='set'>\
         <label style='font-size:14px'>\
         <input type='checkbox' name='ai_audit_log' {checked}> {lbl}</label> \
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:7px 14px;font-size:13px;cursor:pointer;margin-left:8px'>{save}</button></form>\
         <form method='post' action='/open-logs' style='margin-top:8px'>\
         <button type='submit' style='background:#6e7781;color:#fff;border:0;border-radius:8px;\
         padding:7px 14px;font-size:13px;cursor:pointer'>{open}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note} <code>{dir}</code></div>",
        h = t("AI audit log", "AI 監査ログ"),
        lbl = t(
            "Save what is sent to / returned from the AI",
            "AI に送った内容・返ってきた内容を保存する"
        ),
        save = t("Save", "保存"),
        open = t("📂 Open logs folder", "📂 ログフォルダを開く"),
        note = t(
            "Saves the exact request, the raw response, and the masking table (real → masked) \
             per run. Logs contain your real local paths, so they stay on your machine — \
             nothing is uploaded. Off by default.",
            "実行ごとに「実際の送信内容・生のレスポンス・マスキング表（実パス→送信値）」を保存します。\
             ログには実パスが含まれるため端末内にのみ保存され、どこにもアップロードしません。既定はオフ。"
        ),
    )
}

/// A concrete "≈ N points kept" line for the chosen interval + retention, so the
/// coupling is visible: interval is resolution, retention is how far back.
fn trend_hint() -> String {
    match crate::trend::interval_minutes() {
        None => t(
            "Periodic recording is off — only a scan adds a point.",
            "定期記録はオフ — スキャンした時だけ点が増えます。",
        )
        .to_string(),
        Some(mins) => {
            let per_day = (1440 / mins.max(1)).max(1);
            match crate::trend::retention_days() {
                Some(days) => {
                    let pts = per_day * days;
                    let size = crate::util::human(pts * crate::trend::BYTES_PER_POINT);
                    if is_ja() {
                        format!("≈ {pts} 点を保持（{mins}分ごと × {days}日分）・ディスク約 {size}")
                    } else {
                        format!(
                            "≈ {pts} points kept ({mins} min × {days} days) · about {size} on disk"
                        )
                    }
                }
                None => {
                    let cap = crate::trend::MAX_POINTS as u64;
                    let max_size = crate::util::human(cap * crate::trend::BYTES_PER_POINT);
                    let days_to_cap = (cap / per_day).max(1);
                    if is_ja() {
                        format!(
                            "1日あたり約 {per_day} 点を無期限に保持 — 上限 {cap} 点（ディスク最大約 {max_size}・約 {days_to_cap} 日で上限到達）"
                        )
                    } else {
                        format!(
                            "~{per_day} points/day, kept forever — cap {cap} points (up to ~{max_size} on disk, ~{days_to_cap} days to reach it)"
                        )
                    }
                }
            }
        }
    }
}

/// The trend / periodic-sampling ("定点観測") section: two independent knobs —
/// interval = how often a point is recorded (resolution), retention = how far
/// back history is kept. Changing the interval does NOT change the period.
fn trend_section() -> String {
    let cur_int = read_key("trend_interval"); // "" → default 30
    let cur_ret = read_key("trend_retention"); // "" → default 21
    let radio = |name: &str, val: &str, label: &str, cur: &str, default: &str| {
        let checked = if val == cur || (cur.is_empty() && val == default) {
            "checked"
        } else {
            ""
        };
        format!(
            "<label style='margin-right:14px;font-size:14px;white-space:nowrap'>\
             <input type='radio' name='{name}' value='{val}' {checked}> {label}</label>"
        )
    };
    let i = |v: &str, l: &str| radio("trend_interval", v, l, &cur_int, "30");
    let r = |v: &str, l: &str| radio("trend_retention", v, l, &cur_ret, "21");
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <form method='post' action='/settings'>\
         <input type='hidden' name='trend_action' value='set'>\
         <div style='font-size:13px;color:#57606a;margin-bottom:4px'>{int_l}</div>\
         <div style='margin-bottom:10px'>{}{}{}{}</div>\
         <div style='font-size:13px;color:#57606a;margin-bottom:4px'>{ret_l}</div>\
         <div style='margin-bottom:10px'>{}{}{}{}{}</div>\
         <div style='font-size:12px;color:#57606a;margin-bottom:10px'>📊 {hint}</div>\
         <button type='submit' style='background:#1f6feb;color:#fff;border:0;border-radius:8px;\
         padding:8px 16px;font-size:14px;cursor:pointer'>{save}</button></form>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div>",
        i("15", t("15 min", "15分")),
        i("30", t("30 min", "30分")),
        i("60", t("1 hour", "1時間")),
        i("off", t("Off", "オフ")),
        r("7", t("7 days", "7日")),
        r("21", t("21 days", "21日")),
        r("30", t("1 month", "1ヶ月")),
        r("365", t("1 year", "1年")),
        r("forever", t("Forever", "無期限")),
        h = t("📈 Trend recording", "📈 推移の記録（定点観測）"),
        int_l = t(
            "Interval — how often a free-space point is recorded (resolution):",
            "記録間隔 — 空き容量を記録する頻度（＝解像度）:"
        ),
        ret_l = t(
            "Retention — how far back history is kept (independent of interval):",
            "保持期間 — どこまで遡れるか（間隔とは独立）:"
        ),
        hint = trend_hint(),
        save = t("Save", "保存"),
        note = t(
            "Recorded only while DiskSage is running; each point is just timestamp + \
             free/total bytes (no file names or paths), stored locally. The interval sets \
             resolution; retention sets the period — changing the interval does not change \
             how far back you can look. Each scan still adds a special ◆ point on the chart.",
            "DiskSage 常駐中のみ記録します。1点は「時刻＋空き/全体バイト数」だけ（ファイル名やパスは含みません）でローカル保存。間隔＝解像度、保持期間＝遡れる長さで、間隔を変えても遡れる期間は変わりません。スキャンした時はチャートに ◆ の特別な点が追加されます。"
        ),
    )
}

/// The startup section: informational only. The actual login-item toggle lives in
/// the native menu bar (tray), because this web UI runs on localhost and can't call
/// the OS auto-start API — so we point the user there instead of a dead control.
fn startup_section() -> String {
    format!(
        "<h3 style='font-size:15px;margin:24px 0 6px'>{h}</h3>\
         <div style='color:#57606a;font-size:13px;line-height:1.6'>{note}</div>",
        h = t("Start at login", "ログイン時に起動"),
        note = t(
            "To launch DiskSage automatically at login, use the menu bar icon → “Start at Login”. \
             (It's a native setting, so it isn't on this page.)",
            "ログイン時に DiskSage を自動起動するには、メニューバーのアイコン →「ログイン時に起動」を選んでください。\
             （OS ネイティブの設定のため、この画面にはありません。）"
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
         {trend}{provider}{ai}{ollama}{audit}{update}{startup}\
         <h3 style='font-size:15px;margin:24px 0 6px'>{about}</h3>\
         <div style='color:#57606a;font-size:13px;line-height:1.7'>{data_l} <code>{}</code><br>{never}</div>",
        opt("", t("Auto (system)", "自動（システム）")),
        opt("ja", "日本語"),
        opt("en", "English"),
        esc(&data_dir),
        trend = trend_section(),
        provider = provider_section(),
        ai = ai_section(),
        ollama = ollama_section(),
        audit = audit_section(),
        update = update_section(),
        startup = startup_section(),
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
