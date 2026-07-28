//! Persona and managed-agent command request types, split from `types.rs`
//! (file-size cap).

use std::collections::BTreeMap;

use serde::Deserialize;

use super::{
    default_start_on_app_launch, normalize_capability_policy, validate_capability_policy,
    validate_respond_to_allowlist, AgentCapabilityPolicy, AgentDefinition, BackendKind,
    RelayMeshConfig, RespondTo, SkillPolicy,
};

/// The NIP-AP behavioral group as one grouped request field.
///
/// Grouped (not flat) because `update_persona` has legacy callers that don't
/// send behavioral fields at all — flat replace semantics would silently wipe
/// a stored behavior group on every team-import edit. Absent group = don't touch the
/// stored behavior group; present group = validate and replace the fields as a unit
/// (mode and allowlist must travel together).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaBehaviorRequest {
    #[serde(default)]
    pub respond_to: Option<RespondTo>,
    #[serde(default)]
    pub respond_to_allowlist: Vec<String>,
    #[serde(default)]
    pub parallelism: Option<u32>,
}

/// Validate a behavior group and apply it onto a persona record.
///
/// This is the single write path for definition behavioral fields — both
/// `create_persona` and `update_persona` route through it, so neither can
/// skip validation. `None` leaves the record's stored behavior group untouched (the
/// legacy-caller wipe hazard); `Some` normalizes the allowlist
/// (`validate_respond_to_allowlist`), rejects allowlist mode with an empty
/// list (the spawn-time crash-loop `build_respond_to_env` errors on), rejects
/// out-of-range parallelism, and stores the behavior group in wire shape.
pub fn apply_persona_behavior(
    record: &mut AgentDefinition,
    behavior: Option<PersonaBehaviorRequest>,
) -> Result<(), String> {
    let Some(behavior) = behavior else {
        return Ok(());
    };

    let allowlist = validate_respond_to_allowlist(&behavior.respond_to_allowlist)?;
    if behavior.respond_to == Some(RespondTo::Allowlist) && allowlist.is_empty() {
        return Err(
            "respond-to mode 'allowlist' requires at least one pubkey in the allowlist".to_string(),
        );
    }
    if let Some(count) = behavior.parallelism {
        if !(1..=32).contains(&count) {
            return Err(format!(
                "parallelism {count} is out of range (must be between 1 and 32)"
            ));
        }
    }

    record.respond_to = behavior.respond_to.map(|mode| mode.as_str().to_string());
    // The allowlist only means something in allowlist mode; storing it for
    // other modes would republish stale pubkeys the author didn't choose.
    record.respond_to_allowlist = if behavior.respond_to == Some(RespondTo::Allowlist) {
        allowlist
    } else {
        Vec::new()
    };
    record.parallelism = behavior.parallelism;
    Ok(())
}

/// The composed-prompt cap guard (SPEC-004): a prospective base prompt plus
/// the policy's skill sections must fit the 128 KiB composed cap on EVERY
/// save path — including prompt-only edits where the policy request field is
/// absent (the stored `Selected`-skills policy still composes against the
/// new prompt). Compose re-checks at resolve time as defense in depth; this
/// surfaces the failure at the save boundary (plan §07 row 5).
pub fn validate_persona_composed_prompt(
    system_prompt: &str,
    policy: &AgentCapabilityPolicy,
) -> Result<(), String> {
    let SkillPolicy::Selected { selected } = &policy.skills else {
        return Ok(());
    };
    let sections = crate::managed_agents::prompt_skills::compose_skill_sections(selected)?;
    let composed = system_prompt.len() + sections.len();
    if composed > crate::managed_agents::prompt_skills::MAX_COMPOSED_PROMPT_BYTES {
        return Err(format!(
            "system prompt plus selected skills exceeds the {} byte prompt limit ({composed} bytes)",
            crate::managed_agents::prompt_skills::MAX_COMPOSED_PROMPT_BYTES
        ));
    }
    Ok(())
}

/// Validate a capability policy group and apply it onto a persona record.
///
/// Same absent-vs-present group contract as [`apply_persona_behavior`]:
/// `None` leaves the stored policy untouched (legacy callers don't send it);
/// `Some` validates (`validate_capability_policy` — non-empty selections,
/// known skill ids, byte caps), normalizes (dedupe preserving order), checks
/// the composed final prompt fits the 128 KiB cap, and replaces the group as
/// a unit. Team personas (`source_team`) are non-editable: the policy is
/// locked exactly like system_prompt/model there.
pub fn apply_persona_capability_policy(
    record: &mut AgentDefinition,
    capability_policy: Option<AgentCapabilityPolicy>,
) -> Result<(), String> {
    let Some(mut policy) = capability_policy else {
        return Ok(());
    };
    if record.source_team.is_some() {
        return Err("team personas are non-editable: capability policy is locked".to_string());
    }
    validate_capability_policy(&policy)?;
    normalize_capability_policy(&mut policy);
    validate_persona_composed_prompt(&record.system_prompt, &policy)?;
    record.capability_policy = policy;
    Ok(())
}

/// [`apply_persona_capability_policy`] plus the save-time runtime
/// compatibility check (HC-003): the applied policy must be honor-able by
/// the persona's prospective runtime. Create/update persona call sites share
/// this single sequence so the two steps can never drift apart; the spawn
/// descriptor's typed Err remains the backstop.
pub fn apply_persona_capability_policy_checked(
    record: &mut AgentDefinition,
    capability_policy: Option<AgentCapabilityPolicy>,
) -> Result<(), String> {
    apply_persona_capability_policy(record, capability_policy)?;
    // SPEC-004: the composed-prompt cap also guards saves where the policy
    // field is ABSENT — a prompt-only edit still composes the stored
    // `Selected`-skills policy against the new prompt, and an over-cap save
    // would otherwise surface later as an InvalidPolicy spawn/deploy refusal
    // (failure at the wrong boundary).
    validate_persona_composed_prompt(&record.system_prompt, &record.capability_policy)?;
    crate::managed_agents::capability_compiler::validate_policy_against_runtime(
        &record.capability_policy,
        record.runtime.as_deref(),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePersonaRequest {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub system_prompt: String,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub name_pool: Vec<String>,
    /// Environment variables for agents created from this persona.
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    /// NIP-AP behavioral group. Absent = behavior group stays unset.
    #[serde(default)]
    pub behavior: Option<PersonaBehaviorRequest>,
    /// Capability policy group. Absent = policy stays unset (harness defaults).
    #[serde(default)]
    pub capability_policy: Option<AgentCapabilityPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePersonaRequest {
    pub id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub system_prompt: String,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub name_pool: Vec<String>,
    /// Environment variables for agents created from this persona.
    ///
    /// Absent (`None`) = don't touch the stored value (caller didn't include
    /// the field). `Some(map)` = replace entirely (empty map clears all).
    /// Defaulting an omitted field to an empty map would silently erase
    /// stored credentials when an unrelated field is edited.
    #[serde(default)]
    pub env_vars: Option<BTreeMap<String, String>>,
    /// NIP-AP behavioral group. Same absent-vs-present contract as `env_vars`:
    /// absent = don't touch the stored behavior group (legacy callers don't send it),
    /// present = validate and replace the fields as a unit.
    #[serde(default)]
    pub behavior: Option<PersonaBehaviorRequest>,
    /// Capability policy group. Same contract: absent = don't touch; present =
    /// validate and replace as a unit.
    #[serde(default)]
    pub capability_policy: Option<AgentCapabilityPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManagedAgentRequest {
    pub name: String,
    #[serde(default)]
    pub persona_id: Option<String>,
    /// Optional deployment-time team binding for runtime instruction layering.
    #[serde(default)]
    pub team_id: Option<String>,
    pub relay_url: Option<String>,
    pub acp_command: Option<String>,
    pub agent_command: Option<String>,
    /// True when `agent_command` is a runtime command the user deliberately
    /// picked for a linked persona. Distinguishes a real selection, including an
    /// installed alias, from a missing-runtime fallback so a persona-backed
    /// create only stores an `agent_command_override` for the former.
    #[serde(default)]
    pub harness_override: bool,
    #[serde(default)]
    pub agent_args: Vec<String>,
    /// Accepted for wire compatibility; not applied to the record. The
    /// effective MCP command is always derived from the runtime catalog at
    /// spawn time — a per-record override is never read.
    ///
    /// @deprecated — sending this field has no effect.
    #[allow(dead_code)]
    pub mcp_command: Option<String>,
    /// Accepted for wire compatibility; not applied to the record.
    /// `BUZZ_ACP_TURN_TIMEOUT` is deprecated and ignored by the harness.
    ///
    /// @deprecated — sending this field has no effect.
    #[allow(dead_code)]
    pub turn_timeout_seconds: Option<u64>,
    pub idle_timeout_seconds: Option<u64>,
    pub max_turn_duration_seconds: Option<u64>,
    pub parallelism: Option<u32>,
    pub system_prompt: Option<String>,
    pub avatar_url: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    /// Environment variables for this agent. Layered on top of persona env.
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub spawn_after_create: bool,
    #[serde(default = "default_start_on_app_launch")]
    pub start_on_app_launch: bool,
    #[serde(default)]
    pub backend: BackendKind,
    /// `None` = caller expressed no preference: the definition's
    /// `respond_to` default applies when linked, `RespondTo::default()`
    /// otherwise. `Some` is an explicit instance-level choice and always
    /// wins over the definition default.
    #[serde(default)]
    pub respond_to: Option<RespondTo>,
    /// Raw allowlist as received from the frontend. Validated and normalized
    /// before being written to the record.
    #[serde(default)]
    pub respond_to_allowlist: Vec<String>,
    #[serde(default)]
    pub relay_mesh: Option<RelayMeshConfig>,
    /// Capability policy for this agent at mint. `None` = inherit the linked
    /// definition's policy (or harness defaults when definition-less);
    /// `Some` = validated and stored as the instance override.
    #[serde(default)]
    pub capability_policy: Option<AgentCapabilityPolicy>,
}

/// Patch request for updating a managed agent's mutable fields.
///
/// Tri-state nullable semantics via `Option<Option<T>>`:
/// - Field absent in JSON → `None` (don't touch)
/// - `"field": null` → `Some(None)` (clear to default)
/// - `"field": "value"` → `Some(Some("value"))` (set)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManagedAgentRequest {
    pub pubkey: String,
    /// Absent = don't touch. Present = rename the agent.
    #[serde(default)]
    pub name: Option<String>,
    /// Absent = don't touch. null = clear to agent default. "id" = set.
    #[serde(default)]
    pub model: Option<Option<String>>,
    #[serde(default)]
    pub system_prompt: Option<Option<String>>,
    /// Absent = don't touch. Present = replace the env_vars map entirely.
    #[serde(default)]
    pub env_vars: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub parallelism: Option<u32>,
    /// Accepted for wire compatibility; not applied to the stored record.
    /// `BUZZ_ACP_TURN_TIMEOUT` is deprecated and ignored by the harness.
    ///
    /// @deprecated — sending this field has no effect.
    #[allow(dead_code)]
    #[serde(default)]
    pub turn_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub acp_command: Option<String>,
    #[serde(default)]
    pub agent_command: Option<String>,
    /// True when the accompanying `agent_command` is a runtime/Custom command
    /// the user deliberately picked for a linked persona (i.e. the dialog is
    /// not inheriting). Distinguishes a real pin — including one that maps to
    /// the persona's own runtime — from a persona-authoritative restatement,
    /// so a same-runtime pick is preserved instead of being dropped back to
    /// inherit. Ignored when `agent_command` is absent or the inherit sentinel.
    #[serde(default)]
    pub harness_override: bool,
    #[serde(default)]
    pub agent_args: Option<Vec<String>>,
    /// Accepted for wire compatibility; not applied to the stored record.
    /// The effective MCP command is always catalog-derived at spawn time.
    ///
    /// @deprecated — sending this field has no effect.
    #[allow(dead_code)]
    #[serde(default)]
    pub mcp_command: Option<String>,
    /// Absent = don't touch. null = clear to runtime default. "id" = set.
    #[serde(default, deserialize_with = "crate::util::double_option")]
    pub provider: Option<Option<String>>,
    /// Absent = don't touch. Present = set mode.
    #[serde(default)]
    pub respond_to: Option<RespondTo>,
    /// Absent = don't touch. Present = replace the allowlist (validated &
    /// normalized server-side).
    #[serde(default)]
    pub respond_to_allowlist: Option<Vec<String>>,
    /// Tri-state capability policy override: absent = don't touch, null =
    /// clear to inherit the definition (or defaults), value = validate and
    /// set as the instance override.
    #[serde(default, deserialize_with = "crate::util::double_option")]
    pub capability_policy_override: Option<Option<AgentCapabilityPolicy>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::managed_agents::{SkillPolicy, ToolCapabilityId, ToolPolicy};

    fn record_with_quad() -> AgentDefinition {
        let mut record = record_without_quad();
        record.respond_to = Some("allowlist".to_string());
        record.respond_to_allowlist = vec!["a".repeat(64)];
        record.parallelism = Some(4);
        record
    }

    fn record_without_quad() -> AgentDefinition {
        AgentDefinition {
            id: "p-1".to_string(),
            display_name: "Test".to_string(),
            avatar_url: None,
            system_prompt: "prompt".to_string(),
            runtime: None,
            model: None,
            provider: None,
            name_pool: Vec::new(),
            is_builtin: false,
            is_active: true,
            source_team: None,
            source_team_persona_slug: None,
            env_vars: BTreeMap::new(),
            respond_to: None,
            respond_to_allowlist: Vec::new(),
            parallelism: None,
            capability_policy: AgentCapabilityPolicy::default(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn record_with_policy() -> AgentDefinition {
        let mut record = record_without_quad();
        record.capability_policy = AgentCapabilityPolicy {
            tools: ToolPolicy::Selected {
                selected: vec![ToolCapabilityId::FilesRead],
            },
            skills: SkillPolicy::None,
        };
        record
    }

    /// Anchor row mirroring `absent_behavior_leaves_stored_quad_untouched`:
    /// an absent capability policy group must leave the stored group
    /// untouched — legacy callers send no policy field and must not wipe it.
    #[test]
    fn absent_capability_policy_leaves_stored_policy_untouched() {
        let mut record = record_with_policy();
        apply_persona_capability_policy(&mut record, None).unwrap();
        assert_eq!(
            record.capability_policy.tools,
            ToolPolicy::Selected {
                selected: vec![ToolCapabilityId::FilesRead]
            }
        );
        assert_eq!(record.capability_policy.skills, SkillPolicy::None);
    }

    #[test]
    fn present_capability_policy_replaces_as_a_unit() {
        let mut record = record_with_policy();
        apply_persona_capability_policy(
            &mut record,
            Some(AgentCapabilityPolicy {
                tools: ToolPolicy::None,
                skills: SkillPolicy::Selected {
                    selected: vec!["buzz-cli".to_string()],
                },
            }),
        )
        .unwrap();
        assert_eq!(record.capability_policy.tools, ToolPolicy::None);
        assert_eq!(
            record.capability_policy.skills,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()]
            }
        );
    }

    #[test]
    fn empty_selected_policy_is_rejected() {
        let mut record = record_without_quad();
        let err = apply_persona_capability_policy(
            &mut record,
            Some(AgentCapabilityPolicy {
                tools: ToolPolicy::Selected { selected: vec![] },
                skills: SkillPolicy::default(),
            }),
        )
        .unwrap_err();
        assert!(err.contains("at least one"), "{err}");
        // Rejection must not half-apply: the record stays untouched.
        assert!(record.capability_policy.is_default());

        let err = apply_persona_capability_policy(
            &mut record,
            Some(AgentCapabilityPolicy {
                tools: ToolPolicy::default(),
                skills: SkillPolicy::Selected { selected: vec![] },
            }),
        )
        .unwrap_err();
        assert!(err.contains("at least one"), "{err}");
    }

    #[test]
    fn unknown_skill_id_is_rejected_by_name() {
        let mut record = record_without_quad();
        let err = apply_persona_capability_policy(
            &mut record,
            Some(AgentCapabilityPolicy {
                tools: ToolPolicy::default(),
                skills: SkillPolicy::Selected {
                    selected: vec!["not-a-skill".to_string()],
                },
            }),
        )
        .unwrap_err();
        assert!(err.contains("not-a-skill"), "{err}");
    }

    #[test]
    fn selected_ids_are_deduped_preserving_order() {
        let mut record = record_without_quad();
        apply_persona_capability_policy(
            &mut record,
            Some(AgentCapabilityPolicy {
                tools: ToolPolicy::Selected {
                    selected: vec![
                        ToolCapabilityId::FilesWrite,
                        ToolCapabilityId::FilesRead,
                        ToolCapabilityId::FilesWrite,
                    ],
                },
                skills: SkillPolicy::default(),
            }),
        )
        .unwrap();
        assert_eq!(
            record.capability_policy.tools,
            ToolPolicy::Selected {
                selected: vec![ToolCapabilityId::FilesWrite, ToolCapabilityId::FilesRead]
            }
        );
    }

    #[test]
    fn team_persona_policy_is_locked() {
        let mut record = record_with_policy();
        record.source_team = Some("team-1".to_string());

        let err =
            apply_persona_capability_policy(&mut record, Some(AgentCapabilityPolicy::default()))
                .unwrap_err();
        assert!(err.contains("non-editable"), "{err}");
    }

    /// SPEC-004: the 128 KiB composed-prompt cap fires on EVERY persona save
    /// — including a prompt-only edit where the policy request field is
    /// absent but the stored `Selected`-skills policy still composes against
    /// the new prompt. Without it the save succeeds and spawn/deploy refuse
    /// later with InvalidPolicy (failure at the wrong boundary, §5/§07 row 5).
    #[test]
    fn prompt_only_edit_over_the_composed_cap_is_rejected() {
        use crate::managed_agents::prompt_skills::MAX_COMPOSED_PROMPT_BYTES;

        let mut record = record_without_quad();
        // Store a Selected-skills policy while the prompt is small.
        apply_persona_capability_policy_checked(
            &mut record,
            Some(AgentCapabilityPolicy {
                skills: SkillPolicy::Selected {
                    selected: vec!["buzz-cli".to_string()],
                },
                ..Default::default()
            }),
        )
        .unwrap();

        // Prompt-only save (policy field absent) that pushes the composition
        // over the cap → rejected at the save boundary.
        record.system_prompt = "x".repeat(MAX_COMPOSED_PROMPT_BYTES);
        let err = apply_persona_capability_policy_checked(&mut record, None).unwrap_err();
        assert!(err.contains("prompt limit"), "{err}");

        // A prompt that fits saves cleanly with the stored policy untouched.
        record.system_prompt = "short".to_string();
        apply_persona_capability_policy_checked(&mut record, None).unwrap();
        assert_eq!(
            record.capability_policy.skills,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()]
            }
        );

        // Same check through the present-policy path (prompt grew first,
        // then the group is re-submitted).
        record.system_prompt = "x".repeat(MAX_COMPOSED_PROMPT_BYTES);
        let stored = record.capability_policy.clone();
        let err = apply_persona_capability_policy_checked(&mut record, Some(stored)).unwrap_err();
        assert!(err.contains("prompt limit"), "{err}");
    }

    /// The anchor regression row: an absent behavior group must leave a
    /// stored behavior group untouched — legacy update_persona callers (team import,
    /// profile panel) send no behavior field and must not wipe it.
    #[test]
    fn absent_behavior_leaves_stored_quad_untouched() {
        let mut record = record_with_quad();
        apply_persona_behavior(&mut record, None).unwrap();
        assert_eq!(record.respond_to.as_deref(), Some("allowlist"));
        assert_eq!(record.respond_to_allowlist, vec!["a".repeat(64)]);
        assert_eq!(record.parallelism, Some(4));
    }

    #[test]
    fn present_behavior_replaces_all_four_as_a_unit() {
        let mut record = record_with_quad();
        apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::Anyone),
                respond_to_allowlist: Vec::new(),
                parallelism: None,
            }),
        )
        .unwrap();
        assert_eq!(record.respond_to.as_deref(), Some("anyone"));
        assert!(record.respond_to_allowlist.is_empty());
        assert_eq!(record.parallelism, None);
    }

    #[test]
    fn allowlist_mode_with_empty_list_is_rejected() {
        let mut record = record_without_quad();
        let err = apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::Allowlist),
                respond_to_allowlist: Vec::new(),
                ..Default::default()
            }),
        )
        .unwrap_err();
        assert!(err.contains("allowlist"), "{err}");
        // Rejection must not half-apply: the record stays untouched.
        assert_eq!(record.respond_to, None);
    }

    #[test]
    fn allowlist_entries_are_normalized_via_the_shared_validator() {
        let mut record = record_without_quad();
        let upper = "A".repeat(64);
        apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::Allowlist),
                respond_to_allowlist: vec![upper.clone(), upper],
                ..Default::default()
            }),
        )
        .unwrap();
        // Lowercased and deduplicated, matching the instance-side chokepoint.
        assert_eq!(record.respond_to_allowlist, vec!["a".repeat(64)]);
    }

    #[test]
    fn invalid_allowlist_entry_is_rejected() {
        let mut record = record_without_quad();
        let err = apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::Allowlist),
                respond_to_allowlist: vec!["not-hex".to_string()],
                ..Default::default()
            }),
        )
        .unwrap_err();
        assert!(err.contains("64 hex"), "{err}");
    }

    #[test]
    fn allowlist_is_dropped_for_non_allowlist_modes() {
        let mut record = record_without_quad();
        apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::OwnerOnly),
                respond_to_allowlist: vec!["b".repeat(64)],
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(
            record.respond_to_allowlist.is_empty(),
            "stale pubkeys must not be stored alongside a non-allowlist mode"
        );
    }

    /// Pinky's loop row: an applied behavior group must flow through
    /// `persona_event_content` so the republished 30175 carries the edited
    /// behavior group — the write path and the publish path cannot drift apart.
    #[test]
    fn applied_behavior_flows_into_persona_event_content() {
        let mut record = record_without_quad();
        apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                respond_to: Some(RespondTo::Allowlist),
                respond_to_allowlist: vec!["c".repeat(64)],
                parallelism: Some(3),
            }),
        )
        .unwrap();
        let content = crate::managed_agents::persona_events::persona_event_content(&record);
        assert_eq!(content.respond_to.as_deref(), Some("allowlist"));
        assert_eq!(content.respond_to_allowlist, vec!["c".repeat(64)]);
        assert_eq!(content.parallelism, Some(3));
    }

    #[test]
    fn parallelism_out_of_range_is_rejected() {
        let mut record = record_without_quad();
        for bad in [0u32, 33] {
            let err = apply_persona_behavior(
                &mut record,
                Some(PersonaBehaviorRequest {
                    parallelism: Some(bad),
                    ..Default::default()
                }),
            )
            .unwrap_err();
            assert!(err.contains("out of range"), "{err}");
        }

        apply_persona_behavior(
            &mut record,
            Some(PersonaBehaviorRequest {
                parallelism: Some(8),
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(record.parallelism, Some(8));
    }
}
