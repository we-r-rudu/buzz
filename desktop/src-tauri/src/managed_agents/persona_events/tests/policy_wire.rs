//! Capability-policy wire tests, split from `persona_events/tests.rs`
//! (file-size guard): §1.3 policy-on-the-wire hash stability and round-trips,
//! plus the general-005 NIP-AP unknown-id tolerance at the 30175 parse
//! boundary.

use super::{persona_from_event_content_for_test, sample_persona};
use crate::managed_agents::persona_events::{
    persona_content_hash, persona_event_content, PersonaEventContent,
};
use crate::managed_agents::{AgentCapabilityPolicy, SkillPolicy, ToolCapabilityId, ToolPolicy};

// ── Capability policy on the wire (§1.3) ────────────────────────────────────

/// Mirror of `quad_absent_definition_hash_stable_across_activation`: a
/// definition with NO policy bytes must serialize byte-identically to the
/// pre-feature projection, so the feature's activation flips zero fleet
/// hashes and fires zero republish waves.
#[test]
fn policy_absent_persona_hash_stable_across_activation() {
    let record = sample_persona();
    assert!(record.capability_policy.is_default());
    let live = persona_event_content(&record);
    // The pre-feature projection: identical fields, no policy key at all.
    let serialized = serde_json::to_string(&live).unwrap();
    assert!(
        !serialized.contains("capability_policy"),
        "absent policy must not appear in the wire bytes: {serialized}"
    );
    // Round-trip preserves the hash (the drift basis).
    let parsed: PersonaEventContent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(persona_content_hash(&live), persona_content_hash(&parsed));
}

/// A policy edit changes content → the hash flips (linked instances correctly
/// show stale-then-restart); resetting to defaults re-omits the field → the
/// hash returns to baseline and the drift badge clears.
#[test]
fn policy_edit_flips_hash_and_reset_restores_baseline() {
    let baseline_record = sample_persona();
    let baseline = persona_content_hash(&persona_event_content(&baseline_record));

    let mut edited = baseline_record.clone();
    edited.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::Selected {
            selected: vec![ToolCapabilityId::FilesRead],
        },
        skills: SkillPolicy::Inherit,
    };
    let edited_hash = persona_content_hash(&persona_event_content(&edited));
    assert_ne!(baseline, edited_hash, "policy edit must flip the hash");

    let mut reset = edited;
    reset.capability_policy = AgentCapabilityPolicy::default();
    assert_eq!(
        baseline,
        persona_content_hash(&persona_event_content(&reset)),
        "reset to defaults must restore the baseline hash"
    );
}

/// Policy-present projection ↔ `persona_from_event` round-trip: the parsed
/// definition carries the policy, and policy-free events parse to default.
#[test]
fn policy_present_projection_round_trips_through_event() {
    let mut record = sample_persona();
    record.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::None,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    };
    let content = persona_event_content(&record);
    let json = serde_json::to_string(&content).unwrap();
    assert!(json.contains("\"capability_policy\""), "{json}");

    let parsed: PersonaEventContent = serde_json::from_str(&json).unwrap();
    let view = persona_from_event_content_for_test(parsed);
    assert_eq!(view.capability_policy, record.capability_policy);

    // Unknown fields are ignored by old readers (NIP-AP reader contract), and
    // policy-free content parses to the default policy.
    let free: PersonaEventContent = serde_json::from_str(
        &serde_json::to_string(&persona_event_content(&sample_persona())).unwrap(),
    )
    .unwrap();
    assert!(free.capability_policy.is_default());
}

// ── general-005: NIP-AP unknown-id tolerance at the 30175 parse boundary ───
//
// "Readers MUST ignore unknown ids and unknown sub-groups" — a future
// client's capability policy must never make this client reject the whole
// event (losing unrelated prompt/runtime edits too). The storage enum stays
// closed; filtering happens only here, in `deserialize_with`.

#[test]
fn future_tool_id_is_filtered_not_event_fatal() {
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"tools":{"mode":"selected","selected":["files.read","quantum.entangle"]}}}"#,
    )
    .expect("a future tool id must not fail the event parse");
    assert_eq!(
        content.capability_policy.tools,
        ToolPolicy::Selected {
            selected: vec![ToolCapabilityId::FilesRead],
        }
    );
}

#[test]
fn future_skill_id_is_filtered_not_stored() {
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"skills":{"mode":"selected","selected":["buzz-cli","future-skill"]}}}"#,
    )
    .expect("a future skill id must not fail the event parse");
    assert_eq!(
        content.capability_policy.skills,
        SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        }
    );
}

#[test]
fn all_unknown_selection_drops_the_sub_group_to_default() {
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"tools":{"mode":"selected","selected":["quantum.entangle"]},"skills":{"mode":"selected","selected":["future-skill"]}}}"#,
    )
    .unwrap();
    assert!(
        content.capability_policy.is_default(),
        "an all-unknown selection is the protocol's safe state (defaults)"
    );
    // …and therefore re-omits the field on re-publish (absent-stable).
    assert!(
        !serde_json::to_string(&content)
            .unwrap()
            .contains("capability_policy"),
        "a filtered-to-default policy re-omits the field"
    );
}

#[test]
fn unknown_modes_sub_groups_and_malformed_policy_drop_to_default() {
    // Unknown future mode.
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"tools":{"mode":"curated","selected":["files.read"]}}}"#,
    )
    .unwrap();
    assert!(content.capability_policy.is_default());
    // Unknown future sub-group is ignored; known groups still apply.
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"browser_automation":{"level":2},"tools":{"mode":"none"}}}"#,
    )
    .unwrap();
    assert_eq!(content.capability_policy.tools, ToolPolicy::None);
    // Structurally malformed policy (wrong shape) → the whole policy drops,
    // never the event.
    let content: PersonaEventContent =
        serde_json::from_str(r#"{"display_name":"Test","capability_policy":"everything"}"#)
            .unwrap();
    assert!(content.capability_policy.is_default());
    // A malformed sub-group drops in isolation, keeping the known group.
    let content: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"tools":"oops","skills":{"mode":"none"}}}"#,
    )
    .unwrap();
    assert_eq!(content.capability_policy.tools, ToolPolicy::HarnessDefault);
    assert_eq!(content.capability_policy.skills, SkillPolicy::None);
}

#[test]
fn republish_after_filtering_emits_known_ids_only() {
    let parsed: PersonaEventContent = serde_json::from_str(
        r#"{"display_name":"Test","capability_policy":{"tools":{"mode":"selected","selected":["files.read","quantum.entangle"]},"skills":{"mode":"selected","selected":["buzz-cli","future-skill"]}}}"#,
    )
    .unwrap();
    let republished = serde_json::to_string(&parsed).unwrap();
    assert!(republished.contains("files.read"), "{republished}");
    assert!(republished.contains("buzz-cli"), "{republished}");
    assert!(!republished.contains("quantum"), "{republished}");
    assert!(!republished.contains("future-skill"), "{republished}");
}
