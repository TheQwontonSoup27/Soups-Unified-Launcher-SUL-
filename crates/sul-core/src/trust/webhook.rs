//! Discord webhook addresses: check one, and keep its secret out of every place it could leak.
//!
//! A webhook address carries its own password: anyone who has it can post to the channel. This module is the one place that
//! handles such an address. It checks the shape (so a typo or a look-alike site is refused before anything is stored or
//! sent) and wraps the result in [`WebhookUrl`], whose `Debug` and `Display` never print the token.
//!
//! What is NOT here, on purpose: sending anything. Nothing in SUL posts to a webhook, there is no network code, and what
//! would be posted has not been designed (decision 0004). Storing the address in Windows Credential Manager is decided and
//! not built. This module only makes sure the address is handled safely when those exist.

use crate::{Error, Result};
use std::fmt;

/// The hosts Discord serves webhooks from.
const HOSTS: &[&str] = &[
    "discord.com",
    "discordapp.com",
    "ptb.discord.com",
    "canary.discord.com",
];

/// A checked Discord webhook address. The token is reachable only through [`WebhookUrl::expose_secret`].
#[derive(Clone, PartialEq, Eq)]
pub struct WebhookUrl {
    host: String,
    id: String,
    token: String,
}

fn err(why: &str) -> Error {
    // The message never includes the text that was checked: it may be a real address.
    Error::Invalid(format!("not a Discord webhook address: {why}"))
}

impl WebhookUrl {
    /// Check `text`. Accepts exactly `https://<host>/api/webhooks/<id>/<token>` (or `/api/v<n>/webhooks/...`), where `<host>` is
    /// one of Discord's, `<id>` is 17 to 20 digits and `<token>` is letters, digits, `_` and `-`. Anything else is refused:
    /// other schemes (`http`), other hosts and look-alikes, a port, a username or password in the address, a query string
    /// or fragment (thread options are not designed yet), spaces or control characters, and extra path segments.
    pub fn parse(text: &str) -> Result<Self> {
        if text.trim() != text {
            return Err(err("it has spaces around it"));
        }
        if text
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || !c.is_ascii())
        {
            return Err(err("it has a space, control or non-ASCII character"));
        }
        let rest = text
            .strip_prefix("https://")
            .ok_or_else(|| err("it must start with https://"))?;
        if rest.contains(['?', '#', '@', '\\', '%']) {
            return Err(err(
                "it must have no query, fragment, login, backslash or percent-encoding",
            ));
        }
        let mut parts = rest.split('/');
        let host = parts.next().unwrap_or("").to_ascii_lowercase();
        if !HOSTS.contains(&host.as_str()) {
            return Err(err("the host is not one of Discord's"));
        }
        let segments: Vec<&str> = parts.collect();
        let tail = match segments.as_slice() {
            ["api", "webhooks", id, token] => (*id, *token),
            ["api", version, "webhooks", id, token]
                if version.strip_prefix('v').is_some_and(|n| {
                    !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit())
                }) =>
            {
                (*id, *token)
            }
            _ => return Err(err("the path must be /api/webhooks/<id>/<token>")),
        };
        let (id, token) = tail;
        if !(17..=20).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err("the id must be 17 to 20 digits"));
        }
        if token.is_empty()
            || token.len() > 256
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(err("the token must be 1 to 256 letters, digits, _ or -"));
        }
        Ok(Self {
            host,
            id: id.to_owned(),
            token: token.to_owned(),
        })
    }

    /// The webhook's numeric id. It is not a secret on its own.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The address with the token hidden: safe to show and to log.
    pub fn masked(&self) -> String {
        format!("https://{}/api/webhooks/{}/[REDACTED]", self.host, self.id)
    }

    /// The full address, secret included. Named so that every use of it is easy to find; keep the result out of logs,
    /// errors and settings files.
    pub fn expose_secret(&self) -> String {
        format!(
            "https://{}/api/webhooks/{}/{}",
            self.host, self.id, self.token
        )
    }
}

impl fmt::Debug for WebhookUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WebhookUrl({})", self.masked())
    }
}

impl fmt::Display for WebhookUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.masked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::redact_text;

    const ID: &str = "123456789012345678";
    const TOKEN: &str = "aB3_dE-fG7hI9jK1lM5nO7pQ2rS4tU6vW8xY0zA2bC4dE6fG8hI0jK2lM4nO6pQ8rS";

    fn url(host: &str) -> String {
        format!("https://{host}/api/webhooks/{ID}/{TOKEN}")
    }

    #[test]
    fn every_discord_host_and_both_path_shapes_are_accepted() {
        for host in [
            "discord.com",
            "discordapp.com",
            "ptb.discord.com",
            "canary.discord.com",
            "DISCORD.COM",
        ] {
            let w = WebhookUrl::parse(&url(host)).unwrap_or_else(|e| panic!("{host}: {e}"));
            assert_eq!(w.id(), ID);
            assert_eq!(w.expose_secret(), url(&host.to_ascii_lowercase()));
        }
        for v in ["v9", "v10", "v1"] {
            assert!(
                WebhookUrl::parse(&format!(
                    "https://discord.com/api/{v}/webhooks/{ID}/{TOKEN}"
                ))
                .is_ok(),
                "{v}"
            );
        }
        assert!(
            WebhookUrl::parse(&format!(
                "https://discord.com/api/webhooks/{}/x",
                "1".repeat(17)
            ))
            .is_ok()
        );
        assert!(
            WebhookUrl::parse(&format!(
                "https://discord.com/api/webhooks/{}/x",
                "1".repeat(20)
            ))
            .is_ok()
        );
    }

    #[test]
    fn look_alikes_and_wrong_shapes_are_refused() {
        let good = url("discord.com");
        let bad: Vec<String> = vec![
            String::new(),
            "not a url".into(),
            good.replacen("https", "http", 1),
            good.replacen("https://", "", 1),
            format!(" {good}"),
            format!("{good} "),
            format!("{good}\n"),
            format!("{good}?wait=true"),
            format!("{good}#frag"),
            format!("{good}/"),
            format!("{good}/extra"),
            good.replace("discord.com", "discord.com.evil.example"),
            good.replace("discord.com", "evildiscord.com"),
            good.replace("discord.com", "discord.com:8443"),
            good.replace("https://discord.com", "https://user:pass@discord.com"),
            good.replace("https://discord.com", "https://discord.com@evil.example"),
            good.replace("discord.com", "dlscord.com"),
            good.replace("webhooks", "webhook"),
            good.replace("/api/", "/apis/"),
            good.replace("/api/", "/api/v/"),
            good.replace("/api/", "/api/vX/"),
            good.replace("/api/", "/api/v1234/"),
            good.replace(ID, "1234"),
            good.replace(ID, &"1".repeat(21)),
            good.replace(ID, "12345678901234567a"),
            good.replace(TOKEN, ""),
            good.replace(TOKEN, "tok en"),
            good.replace(TOKEN, "tok%2Fen"),
            good.replace(TOKEN, "tok.en"),
            good.replace(TOKEN, &"a".repeat(257)),
            good.replace('/', "\\"),
            format!("https://discord.com/api/webhooks/{ID}/{TOKEN}\u{200b}"),
            "https://дискорд.com/api/webhooks/123456789012345678/abc".into(),
            format!(
                "https://relay.example/send?to=https%3A%2F%2Fdiscord.com%2Fapi%2Fwebhooks%2F{ID}%2F{TOKEN}"
            ),
        ];
        for b in bad {
            assert!(WebhookUrl::parse(&b).is_err(), "{b:?} should be refused");
        }
    }

    #[test]
    fn a_query_a_fragment_a_login_or_percent_encoding_is_refused_with_that_reason() {
        // The later checks would refuse these anyway; this keeps the reason given to the person specific.
        for tail in ["?wait=true", "#x", "%41"] {
            let text = format!("{}{tail}", url("discord.com"));
            let e = WebhookUrl::parse(&text).unwrap_err().to_string();
            assert!(
                e.contains("no query, fragment, login, backslash or percent-encoding"),
                "{tail}: {e}"
            );
        }
        let e = WebhookUrl::parse(&format!(
            "https://user@discord.com/api/webhooks/{ID}/{TOKEN}"
        ))
        .unwrap_err()
        .to_string();
        assert!(e.contains("no query, fragment, login"), "{e}");
    }

    #[test]
    fn the_refusal_never_repeats_what_it_was_given() {
        let secret = url("evil.example");
        let msg = WebhookUrl::parse(&secret).unwrap_err().to_string();
        assert!(
            !msg.contains(TOKEN) && !msg.contains(ID) && !msg.contains("evil.example"),
            "{msg}"
        );
        let msg = WebhookUrl::parse(&format!("{}?x=1", url("discord.com")))
            .unwrap_err()
            .to_string();
        assert!(!msg.contains(TOKEN) && !msg.contains(ID), "{msg}");
    }

    #[test]
    fn debug_display_and_masked_never_show_the_token() {
        let w = WebhookUrl::parse(&url("discord.com")).unwrap();
        let shown = [
            format!("{w:?}"),
            format!("{w}"),
            format!("{w:#?}"),
            w.masked(),
            format!("{:?}", Some(&w)),
            format!("{:?}", vec![w.clone()]),
        ];
        for s in &shown {
            assert!(!s.contains(TOKEN), "{s}");
            assert!(
                s.contains(ID),
                "the id is not secret and helps tell webhooks apart: {s}"
            );
        }
        assert_eq!(
            w.masked(),
            format!("https://discord.com/api/webhooks/{ID}/[REDACTED]")
        );
        assert!(
            w.expose_secret().contains(TOKEN),
            "only the explicitly named method returns the secret"
        );
    }

    #[test]
    fn the_log_redactor_also_catches_this_address_in_every_form_it_can_take() {
        // Two independent protections: this type never shows the token, and the logger masks any webhook-shaped text anyway.
        let w = WebhookUrl::parse(&url("discord.com")).unwrap();
        for text in [
            w.expose_secret(),
            w.masked(),
            format!("{w:?}"),
            format!("failed to post to {w} ({})", w.expose_secret()),
        ] {
            let out = redact_text(&text);
            assert!(!out.contains(TOKEN) && !out.contains(ID), "{out}");
            assert!(out.contains("[REDACTED]"), "{out}");
            assert_eq!(
                redact_text(&out),
                out,
                "redacting twice changes nothing more"
            );
        }
    }

    #[test]
    fn two_addresses_that_differ_only_in_host_case_are_equal() {
        assert_eq!(
            WebhookUrl::parse(&url("DISCORD.com")).unwrap(),
            WebhookUrl::parse(&url("discord.com")).unwrap()
        );
    }
}
