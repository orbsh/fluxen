//! What a page says about its own boot, and which of the two places wins (ADR 0045).
//!
//! Two sources, not three: the address (`?token=`, `?codec=` — the producer can put these on a URL it hands
//! out) and the arguments of `mount(el, opts)`. The call wins, quietly: that is an override, not two
//! declarations of the same intent, so there is nothing to warn about and no precedence arithmetic to keep.
//! Attributes are gone on purpose — a `data-*` vocabulary plus a "which one wins when they disagree" rule
//! existed only because one setting could be said twice; one door removes the question.
//!
//! The resolution is plain data so it has tests that need no browser; the DOM is read by the caller.

use content::codec::CodecType;
use transport::reconnect::{Policy, TransportConfig};

/// The arguments of `mount(el, opts)`. A transport hook is NOT here: a live JS object is not data, so the
/// caller takes it off the same object and hands it to `hook::wrap` before calling in.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Opts {
    pub host: Option<String>,
    pub path: Option<String>,
    pub codec: Option<String>,
    /// Extra parameters for the socket's address. The host decides what they mean; the boot only
    /// joins and escapes them, because the address is the host's fact and not ours (ADR-0046).
    pub query: Option<std::collections::BTreeMap<String, String>>,
    /// The slot this page's options live in (ADR 0038 §2 revised); an empty string means "no slot at all".
    pub config: Option<String>,
    /// The socket's policy — the shape ADR 0044 §2 defines, so one vocabulary for one decision.
    pub policy: Option<TransportConfig>,
}

/// What the boot will actually do.
#[derive(Debug, Clone)]
pub struct Boot {
    /// `None` means "the document's own host" — resolved by the caller, which has a `Location`.
    pub host: Option<String>,
    pub path: String,
    pub codec: CodecType,
    /// The host's extra address parameters, already de-duplicated, in their given order.
    pub query: Vec<(String, String)>,
    /// `None` means "no option slot on this page".
    pub config: Option<String>,
    pub policy: Policy,
    /// Whether the page named a policy — a page that also installed a hook is told when it did.
    pub policy_said: bool,
}

impl Boot {
    /// `Err` is a mount error: a value is present and unusable, which is never downgraded to a default.
    pub fn resolve(opts: Option<&Opts>) -> Result<Self, String> {
        let empty = Opts::default();
        let o = opts.unwrap_or(&empty);

        let codec = match o.codec.as_deref() {
            None => CodecType::Cbor,
            Some(c) => c
                .trim()
                .parse::<CodecType>()
                .map_err(|_| format!("fluxen: codec {c:?} is neither json nor cbor"))?,
        };

        let path = match o.path.as_deref() {
            None => "/channel".to_owned(),
            Some(p) if p.starts_with('/') => p.to_owned(),
            Some(p) => return Err(format!("fluxen: path {p:?} must start with '/'")),
        };

        // An absent `config` keeps the default slot; an empty one says the page has no options at all.
        let config = match o.config.as_deref() {
            None => Some(ui_core::ctx::DEFAULT_CONFIG_SLOT.to_owned()),
            Some(name) if name.trim().is_empty() => None,
            Some(name) => Some(name.trim().to_owned()),
        };

        let policy = match o.policy.as_ref() {
            None => Policy::default(),
            Some(cfg) => cfg
                .policy()
                .map_err(|e| format!("fluxen: opts.policy: {e}"))?,
        };

        Ok(Self {
            host: o.host.clone(),
            path,
            codec,
            query: o.query.clone().unwrap_or_default().into_iter().collect(),
            config,
            policy,
            policy_said: o.policy.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(json: &str) -> Opts {
        serde_json::from_str(json).expect("opts")
    }

    #[test]
    fn nothing_said_is_the_defaults() {
        let b = Boot::resolve(&Query::default(), None).expect("resolve");
        assert_eq!(b.host, None);
        assert_eq!(b.path, "/channel");
        assert_eq!(b.codec, CodecType::Cbor);
        assert_eq!(b.token, None);
        assert_eq!(b.config.as_deref(), Some(ui_core::ctx::DEFAULT_CONFIG_SLOT));
        assert_eq!(b.policy, Policy::default());
        assert!(!b.policy_said);
    }

    #[test]
    fn the_call_wins_over_the_address() {
        let q = Query {
            codec: Some("json".into()),
            token: Some("url".into()),
        };
        let b = Boot::resolve(&q, None).expect("resolve");
        assert_eq!(b.codec, CodecType::Json);
        assert_eq!(b.token.as_deref(), Some("url"));

        let o = opts(r#"{"codec": "cbor", "token": "call"}"#);
        let b = Boot::resolve(&q, Some(&o)).expect("resolve");
        assert_eq!(b.codec, CodecType::Cbor);
        assert_eq!(b.token.as_deref(), Some("call"));
    }

    #[test]
    fn a_field_the_call_omits_keeps_the_address_word() {
        let q = Query {
            codec: Some("json".into()),
            token: Some("url".into()),
        };
        let o = opts(r#"{"path": "/ws"}"#);
        let b = Boot::resolve(&q, Some(&o)).expect("resolve");
        assert_eq!(b.path, "/ws");
        assert_eq!(b.codec, CodecType::Json);
        assert_eq!(b.token.as_deref(), Some("url"));
    }

    #[test]
    fn a_value_that_cannot_be_used_is_an_error_not_a_default() {
        assert!(Boot::resolve(&Query::default(), Some(&opts(r#"{"codec": "protobuf"}"#))).is_err());
        assert!(Boot::resolve(&Query::default(), Some(&opts(r#"{"path": "channel"}"#))).is_err());
        // A policy that contradicts itself is refused where it is read...
        let err = Boot::resolve(
            &Query::default(),
            Some(&opts(r#"{"policy": {"heartbeat": 5000, "dead": 1000}}"#)),
        )
        .unwrap_err();
        assert!(err.contains("dead"), "{err}");
        // ...and a misspelled knob is refused before that, by the options' own shape.
        assert!(serde_json::from_str::<Opts>(r#"{"policy": {"hearbeats": 1}}"#).is_err());
    }

    #[test]
    fn a_policy_in_the_call_is_said_and_parsed() {
        let o = opts(r#"{"policy": {"heartbeat": 0, "retry": false}}"#);
        let b = Boot::resolve(&Query::default(), Some(&o)).expect("resolve");
        assert!(b.policy_said);
        assert_eq!(b.policy.heartbeat, None);
        assert_eq!(b.policy.backoff, None);
    }

    #[test]
    fn an_empty_slot_option_means_no_slot_at_all() {
        let b =
            Boot::resolve(&Query::default(), Some(&opts(r#"{"config": ""}"#))).expect("resolve");
        assert_eq!(b.config, None);
    }

    #[test]
    fn an_unknown_option_key_is_refused() {
        assert!(serde_json::from_str::<Opts>(r#"{"hearbeats": 5000}"#).is_err());
    }
}
