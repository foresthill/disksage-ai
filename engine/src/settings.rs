//! The served Settings screen: report-language selection, read from and written
//! to the same `$DISKSAGE_HOME/config` that the bash CLI and bash serve use, so
//! the setting is consistent across all three.

use std::fs;
use std::path::PathBuf;

use crate::lang::t;
use crate::util::{esc, home_dir};

/// $DISKSAGE_HOME/config (default ~/.disksage/config).
fn config_path() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("config")
}

/// Current `lang=` value ("" if unset → auto-detect).
pub fn read_lang() -> String {
    if let Ok(txt) = fs::read_to_string(config_path()) {
        for line in txt.lines() {
            if let Some(v) = line.strip_prefix("lang=") {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

/// Replace (or, for an empty value, remove) the `lang=` line, preserving others.
pub fn write_lang(val: &str) {
    let path = config_path();
    let mut lines: Vec<String> = Vec::new();
    if let Ok(txt) = fs::read_to_string(&path) {
        for line in txt.lines() {
            if !line.starts_with("lang=") {
                lines.push(line.to_string());
            }
        }
    }
    if !val.is_empty() {
        lines.push(format!("lang={val}"));
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
         <h3 style='font-size:15px;margin:24px 0 6px'>{about}</h3>\
         <div style='color:#57606a;font-size:13px;line-height:1.7'>{data_l} <code>{}</code><br>{never}</div>",
        opt("", t("Auto (system)", "自動（システム）")),
        opt("ja", "日本語"),
        opt("en", "English"),
        esc(&data_dir),
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
