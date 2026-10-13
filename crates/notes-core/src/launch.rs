//! Window launch intentions and an owner-local replay guard; no GUI or storage.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    Main,
    New,
    Note { id: String },
    Quick,
}
impl Intent {
    pub fn validate(&self) -> Result<()> {
        if let Self::Note { id } = self {
            validate_id(id)?;
        }
        Ok(())
    }
}
pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id),
        "Use a lowercase note UUID."
    );
    Ok(())
}

/// Opaque desktop authorization. Do not include its value in logs or errors.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationToken(String);
impl std::fmt::Debug for ActivationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ActivationToken([redacted])")
    }
}
impl ActivationToken {
    pub fn new(value: String) -> Result<Self> {
        let token = Self(value);
        token.validate()?;
        Ok(token)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.0.is_empty()
                && self.0.len() <= 4096
                && !self.0.bytes().any(|b| b == 0 || b.is_ascii_control()),
            "The activation token is empty, too large or malformed."
        );
        Ok(())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    /// A standard platform activation request; the desktop can decline it.
    Requested,
    /// Forwarded token sent to the target Wayland surface; not proof of focus.
    TokenRequested,
    /// Window exists, but the token could not be submitted. Do not force focus.
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opened {
    pub intent: Intent,
    pub note_id: Option<String>,
    pub activation: Activation,
}
pub type Reply = std::result::Result<Opened, String>;

/// Retries of a *recent* request to the same running owner cannot create two
/// windows/notes. This is not a durable capture receipt and cannot be used as one.
#[derive(Default)]
pub struct Recent {
    entries: VecDeque<(String, Intent, Option<ActivationToken>, Option<Reply>)>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    Start,
    Pending,
    Replay(Reply),
    Different,
    Full,
}
impl Recent {
    pub fn admit(
        &mut self,
        id: String,
        intent: Intent,
        token: Option<ActivationToken>,
    ) -> Admission {
        if let Some((_, old, old_token, reply)) = self.entries.iter().find(|e| e.0 == id) {
            if *old != intent || *old_token != token {
                return Admission::Different;
            }
            return reply.clone().map_or(Admission::Pending, Admission::Replay);
        }
        // Only completed entries are evicted. Pending work is bounded by the
        // dispatch queue; it must keep its guard until an outcome is recorded.
        if self.entries.len() >= 128 {
            let Some(index) = self.entries.iter().position(|e| e.3.is_some()) else {
                return Admission::Full;
            };
            self.entries.remove(index);
        }
        self.entries.push_back((id, intent, token, None));
        Admission::Start
    }
    pub fn complete(&mut self, id: &str, reply: Reply) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.0 == id) {
            entry.3 = Some(reply);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_is_scoped_to_the_same_request_and_owner() {
        let mut recent = Recent::default();
        let id = crate::document::new_id();
        assert_eq!(
            recent.admit(id.clone(), Intent::New, None),
            Admission::Start
        );
        assert_eq!(
            recent.admit(id.clone(), Intent::New, None),
            Admission::Pending
        );
        assert_eq!(
            recent.admit(id.clone(), Intent::Quick, None),
            Admission::Different
        );
        let reply = Ok(Opened {
            intent: Intent::New,
            note_id: Some(crate::document::new_id()),
            activation: Activation::Requested,
        });
        recent.complete(&id, reply.clone());
        assert_eq!(
            recent.admit(id.clone(), Intent::New, None),
            Admission::Replay(reply)
        );
        assert_eq!(
            Recent::default().admit(id, Intent::New, None),
            Admission::Start
        );
    }
    #[test]
    fn tokens_are_bounded_validated_and_redacted() {
        for token in [
            String::new(),
            "x".repeat(4097),
            "with\0nul".into(),
            "with\nnewline".into(),
        ] {
            assert!(ActivationToken::new(token).is_err());
        }
        let token = ActivationToken::new("opaque-desktop-token".into()).unwrap();
        assert!(!format!("{token:?}").contains("opaque-desktop-token"));
        assert!(
            Intent::Note {
                id: "not a note".into()
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn recent_requests_are_bounded_without_evicting_pending_work() {
        let mut recent = Recent::default();
        for i in 0..128 {
            assert_eq!(
                recent.admit(i.to_string(), Intent::New, None),
                Admission::Start
            );
        }
        assert_eq!(
            recent.admit("extra".into(), Intent::New, None),
            Admission::Full
        );
        recent.complete("70", Err("Failed before creation".into()));
        assert_eq!(
            recent.admit("extra".into(), Intent::New, None),
            Admission::Start
        );
        assert_eq!(recent.entries.len(), 128);
        assert_eq!(
            recent.admit("0".into(), Intent::New, None),
            Admission::Pending
        );
        assert_eq!(
            recent.admit(
                "0".into(),
                Intent::New,
                Some(ActivationToken::new("different".into()).unwrap())
            ),
            Admission::Different
        );
    }
}
