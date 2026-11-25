//! When a socket is dead, and how long to wait before trying again (ADR 0044).
//!
//! This module is pure arithmetic on purpose: the decision has to be testable without a browser, and the
//! wasm transport only ever asks it "how long do I sleep now".

use serde::Deserialize;

/// The two clocks a live socket keeps: how often to ask "are you there", and how long silence may last
/// before the answer is "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heartbeat {
    /// Send `__ping__` after this much inbound silence. An idle page is the ambiguous case: nothing
    /// arrives because nothing is happening, and nothing arrives because the link is gone.
    pub probe_ms: u64,
    /// Silence longer than this means the link is gone — reconnect. Must be larger than `probe_ms`,
    /// so one unanswered probe is enough to declare it without waiting for two.
    pub dead_ms: u64,
}

impl Default for Heartbeat {
    /// 5 s probe, 10 s dead. The probe is cheap (one empty uplink), and 10 s is the shortest dead window
    /// that survives a stalled main thread — a page blocked in wasm cannot run its own timer, so two
    /// missed rounds are what actually elapse before it notices.
    fn default() -> Self {
        Self {
            probe_ms: 5_000,
            dead_ms: 10_000,
        }
    }
}

impl Heartbeat {
    /// A heartbeat that cannot contradict itself. `dead <= probe` would mean "reconnect before asking",
    /// which is not a policy, it is a bug with a number on it.
    pub fn checked(self) -> Result<Self, String> {
        if self.probe_ms == 0 {
            return Err("heartbeat: probe_ms must be > 0".into());
        }
        if self.dead_ms <= self.probe_ms {
            return Err(format!(
                "heartbeat: dead_ms ({}) must exceed probe_ms ({})",
                self.dead_ms, self.probe_ms
            ));
        }
        Ok(self)
    }
}

/// The page's transport knobs, as one `data-transport` attribute carries them.
///
/// ```text
/// data-transport='{"heartbeat": 5000, "dead": 10000, "retry": {"base": 500, "max": 10000, "jitter": 20}}'
/// data-transport='{"heartbeat": 0}'    no probes: a page that would rather show a stale view than close a
///                                      socket on its own guess
/// data-transport='{"retry": false}'    a socket that is never re-attached: single-session pages, and tests
/// ```
///
/// One attribute, not five, because these numbers are one policy: a template that grows
/// `data-heartbeat` / `data-heartbeat-dead` / `data-backoff-base` / `data-backoff-max` /
/// `data-backoff-jitter` has stopped being readable, and every new knob would add another name.
/// Unknown keys are refused rather than ignored — a misspelled knob that silently does nothing is the
/// failure mode this repository keeps rejecting.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TransportConfig {
    /// Milliseconds of inbound silence before a probe; `0` disables the heartbeat entirely.
    pub heartbeat: u64,
    /// Silence that means the link is gone. `0` means twice `heartbeat` — the pairing that keeps the two
    /// numbers from disagreeing by accident.
    pub dead: u64,
    /// `false` to never reconnect, `true` for the default schedule, or an object to tune it.
    pub retry: Retry,
}

impl Default for TransportConfig {
    fn default() -> Self {
        let hb = Heartbeat::default();
        Self {
            heartbeat: hb.probe_ms,
            dead: 0,
            retry: Retry::Toggle(true),
        }
    }
}

/// The `retry` arm: a switch or a schedule, never both.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum Retry {
    Toggle(bool),
    Spec(BackoffSpec),
}

/// The retry schedule as the page writes it.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BackoffSpec {
    pub base: u64,
    pub max: u64,
    pub jitter: u32,
}

impl Default for BackoffSpec {
    fn default() -> Self {
        let b = Backoff::default();
        Self {
            base: b.base_ms,
            max: b.max_ms,
            jitter: b.jitter_percent,
        }
    }
}

/// What one page's transport should actually do: `None` is "this half is off".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub heartbeat: Option<Heartbeat>,
    pub backoff: Option<Backoff>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            heartbeat: Some(Heartbeat::default()),
            backoff: Some(Backoff::default()),
        }
    }
}

impl TransportConfig {
    /// Read the attribute. Malformed JSON, an unknown key, or a self-contradicting heartbeat is an error the
    /// boot turns into a mount error — never a silently different policy.
    pub fn parse(raw: &str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|e| format!("data-transport: {e}"))
    }

    /// Resolve to the two halves, each optional.
    pub fn policy(&self) -> Result<Policy, String> {
        let heartbeat = if self.heartbeat == 0 {
            None
        } else {
            let dead = if self.dead == 0 {
                self.heartbeat.saturating_mul(2)
            } else {
                self.dead
            };
            Some(
                Heartbeat {
                    probe_ms: self.heartbeat,
                    dead_ms: dead,
                }
                .checked()?,
            )
        };
        let backoff = match self.retry {
            Retry::Toggle(false) => None,
            Retry::Toggle(true) => Some(Backoff::default()),
            Retry::Spec(spec) => Some(Backoff {
                base_ms: spec.base,
                max_ms: spec.max,
                jitter_percent: spec.jitter,
            }),
        };
        Ok(Policy { heartbeat, backoff })
    }
}

/// Exponential backoff with a cap and downward jitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The first wait. Doubles per attempt.
    pub base_ms: u64,
    /// The ceiling: this is what makes the policy a policy instead of a hope. A host that keeps failing
    /// should settle into one retry per cap, not drift to minutes.
    pub max_ms: u64,
    /// Up to this percentage is *subtracted* from the computed wait, so a fleet of pages recovering from
    /// one server restart does not reconnect in lockstep.
    pub jitter_percent: u32,
}

impl Default for Backoff {
    /// 500 ms → 10 s. The first retry is quick enough to feel automatic (a dev restarts the server), and
    /// the cap is the longest a page should take to notice it is back — a minute of waiting is polite to a
    /// server that is really down and rude to a reader whose server came back ten seconds ago.
    fn default() -> Self {
        Self {
            base_ms: 500,
            max_ms: 10_000,
            jitter_percent: 20,
        }
    }
}

impl Backoff {
    /// The wait before attempt `attempt` (1-based). `roll` is the caller's random number in `0..=100`
    /// (wasm has no entropy source here worth trusting; the caller passes what it has).
    ///
    /// `attempt = 1` → `base`, 2 → `2·base`, … capped at `max`; then jitter subtracts up to
    /// `jitter_percent` of that. The result is never below `base / 2`, so a jittered schedule cannot
    /// collapse into a busy loop.
    pub fn wait_ms(&self, attempt: u32, roll: u32) -> u64 {
        let base = self.base_ms.max(1);
        let max = self.max_ms.max(base);
        let shift = attempt.saturating_sub(1).min(32);
        let raw = base.saturating_mul(1u64 << shift).min(max);
        let pct = u64::from(self.jitter_percent.min(100));
        let cut = raw
            .saturating_mul(pct)
            .saturating_mul(u64::from(roll.min(100)))
            / 10_000;
        raw.saturating_sub(cut).max(base / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_until_the_cap() {
        let b = Backoff {
            base_ms: 500,
            max_ms: 4_000,
            jitter_percent: 0,
        };
        let waits: Vec<u64> = (1..=6).map(|n| b.wait_ms(n, 0)).collect();
        assert_eq!(waits, vec![500, 1_000, 2_000, 4_000, 4_000, 4_000]);
    }

    #[test]
    fn jitter_only_subtracts_and_never_goes_to_zero() {
        let b = Backoff::default();
        for attempt in 1..=10 {
            let exact = b.wait_ms(attempt, 0);
            let jittered = b.wait_ms(attempt, 100);
            assert!(jittered <= exact, "attempt {attempt}: {jittered} > {exact}");
            assert!(
                jittered >= b.base_ms / 2,
                "attempt {attempt}: collapsed to {jittered}"
            );
        }
        // 100 % of 20 % of 500 ms is subtracted at the worst roll.
        assert_eq!(b.wait_ms(1, 100), 400);
    }

    #[test]
    fn a_zero_base_cannot_busy_loop() {
        let b = Backoff {
            base_ms: 0,
            max_ms: 0,
            jitter_percent: 0,
        };
        assert_eq!(b.wait_ms(1, 0), 1);
    }

    #[test]
    fn an_absent_attribute_is_the_defaults() {
        let p = TransportConfig::parse("{}")
            .expect("parse")
            .policy()
            .expect("policy");
        assert_eq!(p, Policy::default());
        assert_eq!(
            p.heartbeat.map(|h| (h.probe_ms, h.dead_ms)),
            Some((5_000, 10_000))
        );
    }

    #[test]
    fn zero_switches_the_heartbeat_off_and_false_switches_retry_off() {
        let no_probe = TransportConfig::parse(r#"{"heartbeat": 0}"#).unwrap();
        assert_eq!(no_probe.policy().unwrap().heartbeat, None);

        let no_retry = TransportConfig::parse(r#"{"retry": false}"#).unwrap();
        let p = no_retry.policy().unwrap();
        assert_eq!(p.backoff, None);
        assert!(
            p.heartbeat.is_some(),
            "switching retry off must not touch the heartbeat"
        );

        let both = TransportConfig::parse(r#"{"heartbeat": 0, "retry": false}"#).unwrap();
        assert_eq!(
            both.policy().unwrap(),
            Policy {
                heartbeat: None,
                backoff: None
            }
        );
    }

    #[test]
    fn the_schedule_can_be_written_out() {
        let cfg =
            TransportConfig::parse(r#"{"retry": {"base": 100, "max": 800, "jitter": 0}}"#).unwrap();
        let b = cfg.policy().unwrap().backoff.expect("retry on");
        assert_eq!((b.base_ms, b.max_ms, b.jitter_percent), (100, 800, 0));
        assert_eq!(b.wait_ms(4, 0), 800, "cap honoured");
        assert!(
            TransportConfig::parse(r#"{"retry": true}"#)
                .unwrap()
                .policy()
                .unwrap()
                .backoff
                .is_some()
        );
    }

    #[test]
    fn dead_follows_the_probe_unless_it_is_said() {
        let cfg = TransportConfig::parse(r#"{"heartbeat": 3000}"#).unwrap();
        let hb = cfg.policy().unwrap().heartbeat.unwrap();
        assert_eq!((hb.probe_ms, hb.dead_ms), (3_000, 6_000));

        let explicit = TransportConfig::parse(r#"{"heartbeat": 3000, "dead": 15000}"#).unwrap();
        assert_eq!(
            explicit.policy().unwrap().heartbeat.unwrap().dead_ms,
            15_000
        );
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_default() {
        let err = TransportConfig::parse(r#"{"hearbeats": 5000}"#).unwrap_err();
        assert!(err.contains("hearbeats"), "{err}");
        assert!(TransportConfig::parse("not json").is_err());
        // A dead window inside the probe window is a policy error, caught before anything runs.
        let bad = TransportConfig::parse(r#"{"heartbeat": 5000, "dead": 1000}"#).unwrap();
        assert!(bad.policy().is_err());
    }

    #[test]
    fn the_heartbeat_refuses_to_contradict_itself() {
        assert!(Heartbeat::default().checked().is_ok());
        assert!(
            Heartbeat {
                probe_ms: 5_000,
                dead_ms: 5_000
            }
            .checked()
            .is_err()
        );
        assert!(
            Heartbeat {
                probe_ms: 0,
                dead_ms: 10_000
            }
            .checked()
            .is_err()
        );
    }
}
