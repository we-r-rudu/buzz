//! Live-reload of the inbound author gate from the deployment env file.
//!
//! `BUZZ_ACP_RESPOND_TO` / `BUZZ_ACP_RESPOND_TO_ALLOWLIST` are normally
//! startup-only, so every allowlist change required a service restart —
//! killing in-flight turns. When `--env-file` / `BUZZ_ACP_ENV_FILE` points at
//! the env file the process was launched with (e.g.
//! `/etc/buzz-agents/<name>.env`), [`LiveGate`] stats it on each inbound
//! event and re-parses ONLY those two keys when the mtime changes. The new
//! gate applies to the next event; no restart, no turn disruption.
//!
//! Fail-closed semantics:
//! - a gate key absent from the file falls back to the startup (boot) value;
//! - an unreadable/malformed file, an unknown mode, a mode rejected by
//!   `allowed_respond_to`, a malformed pubkey, or `allowlist` mode with an
//!   empty list keeps the last-good values and logs a warning — a bad edit
//!   never widens the gate.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::SystemTime;

use clap::ValueEnum;

use crate::config::{validate_allowlist, RespondTo};

/// Resolved gate inputs for one inbound event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateValues {
    pub respond_to: RespondTo,
    pub allowlist: HashSet<String>,
}

struct LiveGateState {
    mtime: Option<SystemTime>,
    values: GateValues,
}

/// Mtime-watched view over the two gate keys in the deployment env file.
pub struct LiveGate {
    path: PathBuf,
    boot: GateValues,
    state: RwLock<LiveGateState>,
}

impl LiveGate {
    pub fn new(path: PathBuf, boot: GateValues) -> Self {
        Self {
            path,
            state: RwLock::new(LiveGateState {
                mtime: None,
                values: boot.clone(),
            }),
            boot,
        }
    }

    /// Current gate values, re-parsing the env file when its mtime changed.
    /// The unchanged path costs one `stat` per call.
    pub fn current(&self, allowed_modes: &[String]) -> GateValues {
        let mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        {
            let state = self.state.read().expect("live gate lock poisoned");
            if state.mtime == mtime {
                return state.values.clone();
            }
        }
        let mut state = self.state.write().expect("live gate lock poisoned");
        // Re-check under the write lock — a concurrent event may have reloaded.
        if state.mtime == mtime {
            return state.values.clone();
        }
        match mtime {
            // File missing or unstattable: keep last-good. Advancing the
            // marker to None means we re-check on the next event only via
            // stat, which is the cheap path anyway.
            None => state.mtime = None,
            Some(ts) => {
                state.mtime = Some(ts);
                match std::fs::read_to_string(&self.path) {
                    Ok(content) => match parse_gate_values(&content, &self.boot, allowed_modes) {
                        Ok(values) => {
                            if values != state.values {
                                tracing::info!(
                                    mode = %values.respond_to,
                                    entries = values.allowlist.len(),
                                    path = %self.path.display(),
                                    "live gate reloaded from env file"
                                );
                            }
                            state.values = values;
                        }
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                path = %self.path.display(),
                                "live gate reload failed — keeping last-good values"
                            );
                        }
                    },
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            path = %self.path.display(),
                            "live gate read failed — keeping last-good values"
                        );
                    }
                }
            }
        }
        state.values.clone()
    }
}

/// Parse ONLY the two gate keys from env-file content. Keys absent from the
/// file fall back to the boot values. The merged result is validated as a
/// whole so `allowlist` mode can never pair with an empty list at runtime
/// (mirrors the startup validation in `Config::from_args`).
fn parse_gate_values(
    content: &str,
    boot: &GateValues,
    allowed_modes: &[String],
) -> Result<GateValues, String> {
    let mut respond_to: Option<RespondTo> = None;
    let mut allowlist: Option<HashSet<String>> = None;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        match key.trim() {
            "BUZZ_ACP_RESPOND_TO" => {
                let mode = RespondTo::from_str(value, true)
                    .map_err(|_| format!("invalid BUZZ_ACP_RESPOND_TO value '{value}'"))?;
                if !allowed_modes.is_empty() && !allowed_modes.contains(&mode.to_string()) {
                    return Err(format!(
                        "respond_to '{mode}' is not permitted by BUZZ_ACP_ALLOWED_RESPOND_TO"
                    ));
                }
                respond_to = Some(mode);
            }
            "BUZZ_ACP_RESPOND_TO_ALLOWLIST" => {
                let entries: Vec<String> = value
                    .split(',')
                    .map(|e| e.trim().to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
                allowlist = Some(validate_allowlist(&entries).map_err(|e| e.to_string())?);
            }
            _ => {}
        }
    }
    let values = GateValues {
        respond_to: respond_to.unwrap_or_else(|| boot.respond_to.clone()),
        allowlist: allowlist.unwrap_or_else(|| boot.allowlist.clone()),
    };
    if values.respond_to == RespondTo::Allowlist && values.allowlist.is_empty() {
        return Err("respond_to=allowlist requires a non-empty allowlist".to_string());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PK_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PK_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn boot() -> GateValues {
        GateValues {
            respond_to: RespondTo::OwnerOnly,
            allowlist: HashSet::new(),
        }
    }

    fn temp_env(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "buzz-acp-live-gate-test-{}-{}.env",
            std::process::id(),
            content.len()
        ));
        std::fs::write(&path, content).expect("write temp env");
        path
    }

    #[test]
    fn parse_overrides_both_keys() {
        let content = format!(
            "BUZZ_PRIVATE_KEY=secret\nBUZZ_ACP_RESPOND_TO=allowlist\nBUZZ_ACP_RESPOND_TO_ALLOWLIST={PK_A},{PK_B}\n"
        );
        let values = parse_gate_values(&content, &boot(), &[]).expect("parse");
        assert_eq!(values.respond_to, RespondTo::Allowlist);
        assert_eq!(values.allowlist.len(), 2);
        assert!(values.allowlist.contains(PK_A));
    }

    #[test]
    fn parse_absent_keys_fall_back_to_boot() {
        let content = "BUZZ_PRIVATE_KEY=secret\n";
        let values = parse_gate_values(content, &boot(), &[]).expect("parse");
        assert_eq!(values, boot());
    }

    #[test]
    fn parse_export_prefix_and_quotes() {
        let content = format!("export BUZZ_ACP_RESPOND_TO=\"allowlist\"\nexport BUZZ_ACP_RESPOND_TO_ALLOWLIST='{PK_A}'\n");
        let values = parse_gate_values(&content, &boot(), &[]).expect("parse");
        assert_eq!(values.respond_to, RespondTo::Allowlist);
        assert_eq!(values.allowlist.len(), 1);
    }

    #[test]
    fn parse_rejects_invalid_mode() {
        let err = parse_gate_values("BUZZ_ACP_RESPOND_TO=bogus\n", &boot(), &[]).unwrap_err();
        assert!(err.contains("invalid BUZZ_ACP_RESPOND_TO"), "{err}");
    }

    #[test]
    fn parse_rejects_mode_outside_allowed_set() {
        let allowed = vec!["owner-only".to_string()];
        let content =
            format!("BUZZ_ACP_RESPOND_TO=allowlist\nBUZZ_ACP_RESPOND_TO_ALLOWLIST={PK_A}\n");
        let err = parse_gate_values(&content, &boot(), &allowed).unwrap_err();
        assert!(err.contains("not permitted"), "{err}");
    }

    #[test]
    fn parse_rejects_invalid_pubkey() {
        let err = parse_gate_values("BUZZ_ACP_RESPOND_TO_ALLOWLIST=not-a-pubkey\n", &boot(), &[])
            .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn parse_rejects_allowlist_mode_with_empty_list() {
        let err = parse_gate_values("BUZZ_ACP_RESPOND_TO=allowlist\n", &boot(), &[]).unwrap_err();
        assert!(err.contains("non-empty"), "{err}");
    }

    #[test]
    fn current_reloads_on_change_and_keeps_last_good_on_corruption() {
        let path = temp_env("BUZZ_PRIVATE_KEY=x\n");
        let gate = LiveGate::new(path.clone(), boot());

        // First read: file present but no gate keys → boot values.
        assert_eq!(gate.current(&[]), boot());

        // Edit → allowlist mode picked up without restart.
        std::fs::write(
            &path,
            format!("BUZZ_ACP_RESPOND_TO=allowlist\nBUZZ_ACP_RESPOND_TO_ALLOWLIST={PK_A}\n"),
        )
        .expect("rewrite");
        let values = gate.current(&[]);
        assert_eq!(values.respond_to, RespondTo::Allowlist);
        assert_eq!(values.allowlist.len(), 1);

        // Corrupt → last-good retained.
        std::fs::write(&path, "BUZZ_ACP_RESPOND_TO=bogus\n").expect("corrupt");
        assert_eq!(gate.current(&[]), values);

        // Delete → last-good retained; recreate → boot fallback restored.
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(gate.current(&[]), values);
        std::fs::write(&path, "BUZZ_PRIVATE_KEY=x\n").expect("recreate");
        assert_eq!(gate.current(&[]), boot());

        std::fs::remove_file(&path).ok();
    }
}
