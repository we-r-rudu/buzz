//! Tests for `runtime/summary.rs` — workspace pair-key resolution and the
//! restart-eligibility predicate, split from `runtime/tests.rs` alongside
//! the summary builder they exercise (file-size guard).

// ── workspace pair-key resolution (summary/stop scoping) ────────────────

#[test]
fn unpinned_record_resolves_pair_key_per_workspace() {
    // Community-scoped truth: an unpinned agent running only on relay A must
    // read as running in workspace A and stopped in workspace B — the pair
    // key the summary looks up differs per workspace.
    let pubkey = "aa".repeat(32);
    let key_a = super::resolve_workspace_pair_key(&pubkey, "", "wss://one.example").unwrap();
    let key_b = super::resolve_workspace_pair_key(&pubkey, "", "wss://two.example").unwrap();

    let runtimes = std::collections::HashMap::from([(key_a.clone(), ())]);
    assert!(runtimes.contains_key(&key_a));
    assert!(!runtimes.contains_key(&key_b));
}

#[test]
fn stored_relay_pin_is_ignored_in_pair_key_resolution() {
    // Legacy pins are ignored (#2122): a record carrying a creation-era
    // `relay_url` resolves the same per-workspace pair key an unpinned record
    // does, so summaries/stop act on the community being viewed.
    let pubkey = "aa".repeat(32);
    let from_a =
        super::resolve_workspace_pair_key(&pubkey, "wss://pinned.example", "wss://one.example")
            .unwrap();
    let from_b =
        super::resolve_workspace_pair_key(&pubkey, "wss://pinned.example", "wss://two.example")
            .unwrap();
    assert_ne!(from_a, from_b);
    assert_eq!(from_a.relay_url, "wss://one.example");
    assert_eq!(from_b.relay_url, "wss://two.example");
}

#[test]
fn workspace_pair_key_is_canonical() {
    // Spawn stamps the canonical key; lookup must hit the same entry even
    // when the workspace relay is written in a non-canonical form.
    let pubkey = "aa".repeat(32);
    let stamped = super::resolve_workspace_pair_key(&pubkey, "", "wss://one.example").unwrap();
    let viewed = super::resolve_workspace_pair_key(&pubkey, "", "WSS://One.Example:443/").unwrap();
    assert_eq!(stamped, viewed);
}

#[test]
fn invalid_pubkey_resolves_no_pair_key() {
    // Key-less records (keys minted on first start) cannot form a pair key;
    // the summary must fall back to the stopped/legacy-pid path, not panic.
    assert!(super::resolve_workspace_pair_key("not-a-key", "", "wss://one.example").is_none());
}

// ── restart_eligible tests ──────────────────────────────────────────────

#[test]
fn restart_eligible_true_when_non_orphan_has_hash_drift() {
    assert!(super::restart_eligible(false, true, false));
}

#[test]
fn restart_eligible_true_when_non_orphan_has_availability_drift() {
    assert!(super::restart_eligible(false, false, true));
}

#[test]
fn restart_eligible_false_when_orphan_has_hash_drift() {
    // An orphan can never be restarted successfully — spawn refuses it —
    // so hash drift alone must not surface "Restart required".
    assert!(!super::restart_eligible(true, true, false));
}

#[test]
fn restart_eligible_false_when_orphan_has_availability_drift() {
    assert!(!super::restart_eligible(true, false, true));
}

#[test]
fn restart_eligible_false_when_orphan_has_no_drift() {
    assert!(!super::restart_eligible(true, false, false));
}

#[test]
fn restart_eligible_false_when_non_orphan_has_no_drift() {
    assert!(!super::restart_eligible(false, false, false));
}

// ── SPEC-001: the summary's editable agent_args are the base user vector ──
//
// `build_managed_agent_summary` fills `agent_args` from
// `descriptor.base_args` (never the compiled `descriptor.args`), so the
// Advanced editor round-trips what the user actually entered. The pin lives
// at the descriptor seam: `base_args` must be the pre-compile vector for a
// policy-bearing record, free of capability-flag tokens. Post-HC-001 no v1
// runtime compiles flags, so `args == base_args` — this contract is what the
// next verified transport must preserve.

#[test]
fn policy_bearing_descriptor_exposes_base_args_without_compiled_tokens() {
    use crate::managed_agents::types::{AgentCapabilityPolicy, ManagedAgentRecord, SkillPolicy};

    let mut record: ManagedAgentRecord = serde_json::from_str(
        r#"{
            "pubkey": "abcd1234",
            "name": "test-agent",
            "private_key_nsec": "nsec1fake",
            "relay_url": "wss://localhost:3000",
            "acp_command": "buzz-acp",
            "agent_command": "omp",
            "agent_args": ["--verbose"],
            "runtime": "omp",
            "mcp_command": "",
            "turn_timeout_seconds": 320,
            "system_prompt": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "last_started_at": null,
            "last_stopped_at": null,
            "last_exit_code": null,
            "last_error": null
        }"#,
    )
    .expect("record fixture");
    record.capability_policy_override = Some(AgentCapabilityPolicy {
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
        ..Default::default()
    });
    let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
        &record,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(descriptor.base_args, descriptor.args);
    assert_eq!(descriptor.base_args, vec!["--verbose".to_string()]);
    for token in &descriptor.base_args {
        assert!(!token.starts_with("--tools"), "{token}");
        assert!(!token.starts_with("--skills"), "{token}");
        assert!(!token.starts_with("--no-"), "{token}");
    }

    // An unrelated arg edit (the SPEC-001 round-trip) leaves the base vector
    // exactly as entered — the descriptor still resolves for spawn.
    record.agent_args = vec!["--verbose".to_string(), "--q".to_string()];
    let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
        &record,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        descriptor.base_args,
        vec!["--verbose".to_string(), "--q".to_string()]
    );
}

#[test]
fn summary_agent_args_exposes_base_vector_when_compiled_args_diverge() {
    // round2-general-005: the descriptor-seam pin above cannot catch a
    // summary-projection regression while every production transport is
    // harness-managed (base == compiled everywhere in v1). The next verified
    // transport reintroduces the divergence, which is exactly when a
    // regression to `descriptor.args` would silently persist flattened
    // capability tokens into the editor. This pin fails if the projection
    // ever switches back to the compiled vector.
    let descriptor = crate::managed_agents::readiness::EffectiveHarnessDescriptor {
        command: "fixture-runtime".to_string(),
        args: vec![
            "--tools=read,grep".to_string(),
            "--no-skills".to_string(),
            "acp".to_string(),
        ],
        base_args: vec!["acp".to_string(), "--verbose".to_string()],
        env: Default::default(),
    };

    let projected = super::summary_agent_args(&descriptor);
    assert_eq!(projected, vec!["acp".to_string(), "--verbose".to_string()]);
    for token in &projected {
        assert!(!token.starts_with("--tools"), "{token}");
        assert!(!token.starts_with("--skills"), "{token}");
        assert!(!token.starts_with("--no-"), "{token}");
    }
    // The compiled vector stays available for spawn/hash/deploy — only the
    // summary's editable projection is pinned to the base vector.
    assert!(descriptor.args.iter().any(|a| a.starts_with("--tools=")));
}
