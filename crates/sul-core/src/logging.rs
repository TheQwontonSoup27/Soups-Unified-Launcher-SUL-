//! Structured JSONL logging with secret redaction. Secrets must never reach logs.

use crate::{Result, error::io};
use serde_json::{Map, Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Field names (matched anywhere in the name, in any case) whose value is never written. `webhook` is here
/// because a Discord webhook URL carries its own secret: anyone who has it can post to the channel.
const SENSITIVE: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "authorization",
    "api_key",
    "apikey",
    "cookie",
    "session",
    "webhook",
];

const REDACTED: &str = "[REDACTED]";

pub struct Logger {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

fn is_sensitive(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SENSITIVE.iter().any(|s| k.contains(s))
}

/// Characters that end a URL inside running text: white space, quotes, brackets and commas. Everything
/// else is taken to belong to the URL, so a trailing `)` or `"` is left alone but a query string is not.
fn is_url_char(c: char) -> bool {
    !(c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ','
        ))
}

/// Whether one run of URL characters is a Discord webhook URL (`discord.com`, `discordapp.com`, `ptb.` and
/// `canary.` hosts, any API version). A percent-encoded slash counts as a slash, so a webhook URL passed as
/// the query value of another address is caught too. When in doubt this says yes: a wrongly hidden word
/// costs a little to read, a leaked token cannot be taken back.
fn is_discord_webhook(run: &str) -> bool {
    let plain = run.to_ascii_lowercase().replace("%2f", "/");
    plain.contains("discord") && plain.contains("/webhooks/")
}

fn push_run(out: &mut String, run: &str) {
    out.push_str(if is_discord_webhook(run) {
        REDACTED
    } else {
        run
    });
}

/// `text` with every Discord webhook URL replaced by `[REDACTED]`; everything around it is kept as it was.
/// Use it on any text that may come to hold such a URL before it is written or shown, such as the message of
/// an error from an HTTP request, which usually quotes the address it failed on.
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run_start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_url_char(c) {
            run_start.get_or_insert(i);
        } else {
            if let Some(start) = run_start.take() {
                push_run(&mut out, &text[start..i]);
            }
            out.push(c);
        }
    }
    if let Some(start) = run_start {
        push_run(&mut out, &text[start..]);
    }
    out
}

/// A copy of `v` that is safe to write: values under a sensitive field name are replaced, and any Discord
/// webhook URL inside any text is masked.
pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, val) in m {
                out.insert(
                    k.clone(),
                    if is_sensitive(k) {
                        json!(REDACTED)
                    } else {
                        redact(val)
                    },
                );
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        Value::String(s) => Value::String(redact_text(s)),
        other => other.clone(),
    }
}

impl Logger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(io(p))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io(path))?;
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn log(&self, level: &str, event: &str, fields: Value) -> Result<()> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = json!({
            "ts_unix": ts,
            "level": level,
            "event": redact_text(event),
            "fields": redact(&fields),
        });
        let mut f = self.file.lock().unwrap();
        writeln!(f, "{line}").map_err(io(&self.path))?;
        f.flush().map_err(io(&self.path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made-up values in the shape Discord uses: a numeric id and a long token of letters, digits, `_` and `-`.
    const ID: &str = "123456789012345678";
    const TOKEN: &str = "aB3_dE-fG7hI9jK1lM5nO7pQ2rS4tU6vW8xY0zA2bC4dE6fG8hI0jK2lM4nO6pQ8rS";

    fn url(host: &str) -> String {
        format!("https://{host}/api/webhooks/{ID}/{TOKEN}")
    }

    #[test]
    fn a_webhook_url_alone_is_replaced() {
        assert_eq!(redact_text(&url("discord.com")), "[REDACTED]");
    }

    #[test]
    fn the_words_around_it_are_kept_exactly() {
        let text = format!("posting to {} failed (status 404)", url("discord.com"));
        assert_eq!(
            redact_text(&text),
            "posting to [REDACTED] failed (status 404)"
        );
        let spaced = format!("a  {}\n\tb", url("discord.com"));
        assert_eq!(redact_text(&spaced), "a  [REDACTED]\n\tb");
    }

    #[test]
    fn every_discord_host_and_api_version_is_caught() {
        for host in [
            "discord.com",
            "discordapp.com",
            "ptb.discord.com",
            "canary.discord.com",
            "DISCORD.COM",
        ] {
            let text = format!("x {} y", url(host));
            assert_eq!(redact_text(&text), "x [REDACTED] y", "{host}");
        }
        let versioned = format!("https://discord.com/api/v10/webhooks/{ID}/{TOKEN}");
        assert_eq!(redact_text(&versioned), "[REDACTED]");
        let no_scheme = format!("discord.com/api/webhooks/{ID}/{TOKEN}");
        assert_eq!(redact_text(&no_scheme), "[REDACTED]");
    }

    #[test]
    fn the_token_in_a_query_string_or_after_thread_options_goes_with_it() {
        let with_query = format!("{}?wait=true&thread_id=42", url("discord.com"));
        assert_eq!(redact_text(&with_query), "[REDACTED]");
    }

    #[test]
    fn a_webhook_url_inside_another_address_is_caught_even_when_percent_encoded() {
        let encoded = format!(
            "https://relay.example/send?to=https%3A%2F%2Fdiscord.com%2Fapi%2Fwebhooks%2F{ID}%2F{TOKEN}"
        );
        let out = redact_text(&encoded);
        assert_eq!(out, "[REDACTED]");
        assert!(!out.contains(TOKEN) && !out.contains(ID));
    }

    #[test]
    fn brackets_quotes_and_commas_end_the_url_and_stay() {
        let u = url("discord.com");
        assert_eq!(redact_text(&format!("({u})")), "([REDACTED])");
        assert_eq!(redact_text(&format!("\"{u}\"")), "\"[REDACTED]\"");
        assert_eq!(
            redact_text(&format!("{{\"url\":\"{u}\",\"ok\":true}}")),
            "{\"url\":\"[REDACTED]\",\"ok\":true}"
        );
        assert_eq!(redact_text(&format!("a,{u},b")), "a,[REDACTED],b");
        assert_eq!(redact_text(&format!("<{u}>")), "<[REDACTED]>");
    }

    #[test]
    fn several_urls_in_one_text_are_all_replaced() {
        let text = format!("{} then {}", url("discord.com"), url("ptb.discord.com"));
        assert_eq!(redact_text(&text), "[REDACTED] then [REDACTED]");
    }

    #[test]
    fn other_addresses_and_ordinary_text_are_left_alone() {
        for same in [
            "",
            "nothing to hide here",
            "https://discord.com/channels/1/2",
            "https://discord.com/api/webhooks",
            "https://example.com/webhooks/5/abc",
            "https://github.com/owner/repo/settings/hooks",
            "Discord webhooks are off",
            "naïve — 日本語 text with symbols ✓",
        ] {
            assert_eq!(redact_text(same), same);
        }
    }

    #[test]
    fn redacting_twice_changes_nothing_more() {
        let once = redact_text(&format!("a {} b", url("discord.com")));
        assert_eq!(redact_text(&once), once);
    }

    #[test]
    fn a_value_under_a_webhook_field_name_is_replaced_whatever_it_holds() {
        let fields = json!({
            "webhook_url": "anything",
            "Webhook": 5,
            "discordWebhook": {"id": ID},
            "nested": {"my_webhook_token": "x", "keep": "this"},
        });
        let out = redact(&fields);
        assert_eq!(out["webhook_url"], REDACTED);
        assert_eq!(out["Webhook"], REDACTED);
        assert_eq!(out["discordWebhook"], REDACTED);
        assert_eq!(out["nested"]["my_webhook_token"], REDACTED);
        assert_eq!(out["nested"]["keep"], "this");
    }

    #[test]
    fn a_webhook_url_is_masked_in_any_text_value_at_any_depth() {
        let u = url("discord.com");
        let fields = json!({
            "message": format!("failed to post to {u}"),
            "list": [format!("see {u}"), 3, null, true],
            "deep": {"a": {"b": [{"c": format!("{u}?wait=true")}]}},
            "count": 7,
            "name": "kept",
        });
        let out = redact(&fields);
        assert_eq!(out["message"], "failed to post to [REDACTED]");
        assert_eq!(out["list"], json!(["see [REDACTED]", 3, null, true]));
        assert_eq!(out["deep"]["a"]["b"][0]["c"], REDACTED);
        assert_eq!(out["count"], 7);
        assert_eq!(out["name"], "kept");
        let text = out.to_string();
        assert!(!text.contains(ID) && !text.contains(TOKEN));
    }

    #[test]
    fn the_event_name_is_masked_too() {
        let dir = std::env::temp_dir().join(format!("sul-log-{}", std::process::id()));
        let path = dir.join("event.jsonl");
        let log = Logger::open(&path).unwrap();
        log.log(
            "error",
            &format!("post failed: {}", url("discord.com")),
            json!({}),
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(text.contains("post failed: [REDACTED]"), "{text}");
        assert!(!text.contains(TOKEN) && !text.contains(ID), "{text}");
    }
}
