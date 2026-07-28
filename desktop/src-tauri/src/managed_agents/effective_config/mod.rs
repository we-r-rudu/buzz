use std::collections::BTreeMap;

use serde::Serialize;

use super::global_config::GlobalAgentConfig;
use super::relay_mesh::{
    RELAY_MESH_API_BASE_URL, RELAY_MESH_API_KEY_PLACEHOLDER, RELAY_MESH_AUTO_MODEL_ID,
    RELAY_MESH_PROVIDER_ID,
};
use super::types::{AgentCapabilityPolicy, AgentDefinition, ManagedAgentRecord, SkillPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Definition,
    Global,
    InstanceLegacy,
    /// A linked instance's own `capability_policy_override` won over the
    /// definition's policy (capability resolution only — model/provider/
    /// prompt never use this variant).
    InstanceOverride,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedField<T> {
    pub value: Option<T>,
    pub source: ConfigSource,
}

#[derive(Debug, Clone)]
pub struct EffectiveAgentConfig {
    pub model: ResolvedField<String>,
    pub provider: ResolvedField<String>,
    /// The composed FINAL prompt: persona/base text plus any selected skill
    /// sections (§5). Every delivery consumer — the spawn env write
    /// (`BUZZ_ACP_SYSTEM_PROMPT`), `spawn_config_hash`, and the provider
    /// deploy payload — reads this value, so skills are delivered identically
    /// on all three paths.
    pub system_prompt: ResolvedField<String>,
    /// The raw persona/instance prompt WITHOUT skill sections, same origin
    /// rules as `system_prompt` had pre-feature. Preserved for display needs.
    /// Not consumed yet — kept for the resolved-config contract (§2), same
    /// precedent as `EffectiveAgentEnv::config_file_path`.
    #[allow(dead_code)]
    pub base_system_prompt: ResolvedField<String>,
    /// The resolved capability policy (override → definition → default).
    /// Consumers that need ONLY the policy call
    /// `resolve_effective_capability_policy` directly (descriptor seam); this
    /// field keeps the one-resolve contract complete for display consumers.
    #[allow(dead_code)]
    pub capability_policy: AgentCapabilityPolicy,
}

impl EffectiveAgentConfig {
    /// The relay-mesh model id this config resolves to, or `None` when the
    /// effective provider isn't relay-mesh.
    ///
    /// This is the single authoritative mesh decision for this config.  Both
    /// the mesh preflight (interactive start, restore-on-launch) AND spawn's
    /// `apply_relay_mesh_env` block MUST derive their mesh gate from this
    /// method — never from a separate provider comparison — so the two paths
    /// are guaranteed to agree even when the stored provider string has leading
    /// or trailing whitespace.  The provider is trimmed before matching;
    /// a blank effective model falls back to "auto", mirroring
    /// `apply_relay_mesh_env`'s own rule.
    pub fn relay_mesh_model_id(&self) -> Option<String> {
        if self.provider.value.as_deref().map(str::trim) != Some(RELAY_MESH_PROVIDER_ID) {
            return None;
        }
        Some(
            self.model
                .value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(RELAY_MESH_AUTO_MODEL_ID)
                .to_string(),
        )
    }
}

#[derive(Debug, Clone)]
pub enum EffectiveConfigResult {
    Resolved(EffectiveAgentConfig),
    OrphanedInstance {
        record_pubkey: String,
        missing_persona_id: String,
    },
    /// The stored policy cannot be honored (e.g. an unknown skill id in a
    /// hand-edited store). Spawn/deploy refuse visibly via
    /// `require_resolved`; summary/hash degrade exactly like the orphan arm.
    InvalidPolicy {
        error: String,
    },
}

fn non_blank(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.trim().is_empty())
}

/// The capability policy resolved for a record, with its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCapabilityPolicy {
    pub policy: AgentCapabilityPolicy,
    pub source: ConfigSource,
}

/// Resolve the effective capability policy for `record`:
/// - Linked instance: `capability_policy_override` (Some) → `InstanceOverride`;
///   else the live definition's non-default policy → `Definition`; else the
///   default policy → `Global` (labels the "came from defaults" origin —
///   `GlobalAgentConfig` itself carries NO policy layer).
/// - Definition-less: override (Some) → `InstanceLegacy`; else default.
/// - Orphan: default policy (spawn/deploy refuse the orphan elsewhere via
///   `require_resolved`; the hash degrades like prompt/model/provider).
pub fn resolve_effective_capability_policy(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    _global: &GlobalAgentConfig,
) -> ResolvedCapabilityPolicy {
    if let Some(pid) = &record.persona_id {
        if let Some(definition) = definitions.iter().find(|d| d.id == *pid) {
            if let Some(override_policy) = &record.capability_policy_override {
                return ResolvedCapabilityPolicy {
                    policy: override_policy.clone(),
                    source: ConfigSource::InstanceOverride,
                };
            }
            if !definition.capability_policy.is_default() {
                return ResolvedCapabilityPolicy {
                    policy: definition.capability_policy.clone(),
                    source: ConfigSource::Definition,
                };
            }
        }
        // Orphan (missing definition) or definition with a default policy.
        return ResolvedCapabilityPolicy {
            policy: AgentCapabilityPolicy::default(),
            source: ConfigSource::Global,
        };
    }
    if let Some(override_policy) = &record.capability_policy_override {
        return ResolvedCapabilityPolicy {
            policy: override_policy.clone(),
            source: ConfigSource::InstanceLegacy,
        };
    }
    ResolvedCapabilityPolicy {
        policy: AgentCapabilityPolicy::default(),
        source: ConfigSource::Global,
    }
}

/// Compose the final prompt from the base (persona/instance) prompt and the
/// resolved policy's skill selection. `Inherit`/`None` skills return the
/// base bytes untouched; `Selected` appends deterministic
/// `"\n\n[Skill: {label}]\n{prompt}"` sections (ids sorted ascending).
/// Unknown skill ids fail loudly (a hand-edited store must not silently
/// drop); the composed-final 128 KiB cap is re-checked here as defense in
/// depth behind the save boundary.
///
/// Prefix of the composed-prompt cap error. The inbound kind:30177 boundary
/// ([`downgrade_over_cap_inbound_override`]) gates on it so ONLY the cap
/// triggers a downgrade — an unknown skill id from a hand-edited store must
/// stay the loud InvalidPolicy-at-resolve failure it is locally.
pub(crate) const COMPOSED_CAP_ERROR_PREFIX: &str = "composed prompt exceeds the";
fn compose_final_prompt(
    base_prompt: ResolvedField<String>,
    policy: &AgentCapabilityPolicy,
) -> Result<ResolvedField<String>, String> {
    let SkillPolicy::Selected { selected } = &policy.skills else {
        return Ok(base_prompt);
    };
    let sections = super::prompt_skills::compose_skill_sections(selected)?;
    let base = base_prompt.value.unwrap_or_default();
    let composed_len = base.len() + sections.len();
    if composed_len > super::prompt_skills::MAX_COMPOSED_PROMPT_BYTES {
        return Err(format!(
            "{COMPOSED_CAP_ERROR_PREFIX} {} byte limit ({composed_len} bytes)",
            super::prompt_skills::MAX_COMPOSED_PROMPT_BYTES
        ));
    }
    Ok(ResolvedField {
        value: Some(format!("{base}{sections}")),
        source: base_prompt.source,
    })
}

/// Compose with an injectable catalog — the test seam for fixture skills
/// (mirrors `prompt_skills::compose_skill_sections_from`).
#[cfg(test)]
pub(crate) fn compose_final_prompt_from(
    base_prompt: Option<String>,
    ids: &[super::types::BuzzSkillId],
    catalog: &[super::prompt_skills::BuzzPromptSkill],
) -> Result<Option<String>, String> {
    let sections = super::prompt_skills::compose_skill_sections_from(catalog, ids)?;
    let base = base_prompt.unwrap_or_default();
    let composed_len = base.len() + sections.len();
    if composed_len > super::prompt_skills::MAX_COMPOSED_PROMPT_BYTES {
        return Err(format!(
            "{COMPOSED_CAP_ERROR_PREFIX} {} byte limit ({composed_len} bytes)",
            super::prompt_skills::MAX_COMPOSED_PROMPT_BYTES
        ));
    }
    Ok(Some(format!("{base}{sections}")))
}

fn resolve_linked(
    record: &ManagedAgentRecord,
    definition: &AgentDefinition,
    global: &GlobalAgentConfig,
) -> Result<EffectiveAgentConfig, String> {
    let model = match non_blank(definition.model.as_deref()) {
        Some(m) => ResolvedField {
            value: Some(m.to_owned()),
            source: ConfigSource::Definition,
        },
        None => ResolvedField {
            value: global.model.clone(),
            source: ConfigSource::Global,
        },
    };

    let provider = match non_blank(definition.provider.as_deref()) {
        Some(p) => ResolvedField {
            value: Some(p.to_owned()),
            source: ConfigSource::Definition,
        },
        None => ResolvedField {
            value: global.provider.clone(),
            source: ConfigSource::Global,
        },
    };

    let base_system_prompt = ResolvedField {
        value: non_blank(Some(definition.system_prompt.as_str())).map(str::to_owned),
        source: ConfigSource::Definition,
    };

    let capability_policy =
        resolve_effective_capability_policy(record, std::slice::from_ref(definition), global);
    let system_prompt =
        compose_final_prompt(base_system_prompt.clone(), &capability_policy.policy)?;

    Ok(EffectiveAgentConfig {
        model,
        provider,
        system_prompt,
        base_system_prompt,
        capability_policy: capability_policy.policy,
    })
}

/// The API key value the relay-mesh preset wrote before the Jun-11 rename
/// window (#960, `8f580f308`) changed `RELAY_MESH_API_KEY_PLACEHOLDER` from
/// `"sprout-mesh-local"` to its current value. The old string is persisted as a
/// *value* in `env_vars` on records created before that, and no migration ever
/// rewrote it.
const LEGACY_MESH_API_KEY_PLACEHOLDER: &str = "sprout-mesh-local";

/// The provider key the preset wrote before #971 (`8c8312932`) renamed it to
/// `BUZZ_AGENT_PROVIDER`. That commit changed source literals only — persisted
/// `env_vars` keys were never migrated.
const LEGACY_MESH_PROVIDER_ENV_KEY: &str = "SPROUT_AGENT_PROVIDER";

/// The legacy env discriminator: recognizes the relay-mesh preset purely from
/// the env vars a pre-typed-field record carries, returning its served model id.
///
/// All three sentinels must match — the local base URL alone is not enough,
/// since a user may point their own OpenAI-compatible provider at the same
/// port. The placeholder API key is what makes this Buzz's own preset.
///
/// Two of those sentinels were renamed in the same Jun-11 window, in separate
/// commits, with neither migrating persisted records: the provider env *key*
/// (#971) and the API key *value* (#960). Each is therefore accepted under
/// either spelling, independently — a record straddling the window carries one
/// old and one new. In both cases the current spelling is authoritative when
/// present, so a record that has since been rewritten with a non-mesh value is
/// not resurrected by the stale leftover beside it.
///
/// `OPENAI_COMPAT_BASE_URL` and `OPENAI_COMPAT_MODEL` were never renamed.
/// Nothing beyond these two either/ors is loosened: every sentinel dropped
/// widens the false-positive surface for a user's own openai-compatible agent.
fn mesh_preset_env_model_id(env_vars: &BTreeMap<String, String>) -> Option<String> {
    let base_url = env_vars.get("OPENAI_COMPAT_BASE_URL")?.trim();
    if base_url.trim_end_matches('/') != RELAY_MESH_API_BASE_URL {
        return None;
    }
    let provider = env_vars
        .get("BUZZ_AGENT_PROVIDER")
        .or_else(|| env_vars.get(LEGACY_MESH_PROVIDER_ENV_KEY))?
        .trim();
    if provider != "openai" {
        return None;
    }
    let api_key = env_vars.get("OPENAI_COMPAT_API_KEY")?.trim();
    if api_key != RELAY_MESH_API_KEY_PLACEHOLDER && api_key != LEGACY_MESH_API_KEY_PLACEHOLDER {
        return None;
    }
    non_blank(env_vars.get("OPENAI_COMPAT_MODEL").map(String::as_str)).map(str::to_owned)
}

/// The mesh model id a definition-less record carries in legacy form, or `None`
/// when it is not a legacy mesh record.
///
/// Two shipped record generations predate `provider: "relay-mesh"` and are
/// never rewritten on load, so they are still on disk:
///
/// - the typed `relay_mesh` marker, added before the record had a `provider`
///   field at all;
/// - before that, the mesh preset written straight into `env_vars`.
///
/// Consulted only by [`resolve_definition_less`]: a record with no definition
/// has nothing authoritative to be overridden by, so its own bytes are the
/// highest-precedence signal it has. A linked instance never reaches here.
fn legacy_record_mesh_model_id(record: &ManagedAgentRecord) -> Option<String> {
    match &record.relay_mesh {
        // The marker itself is the mesh signal; a blank `model_ref` still means
        // mesh, resolved to the auto model exactly as `apply_relay_mesh_env`
        // and `relay_mesh_model_id` treat a blank model.
        Some(config) => Some(
            non_blank(Some(config.model_ref.as_str()))
                .unwrap_or(RELAY_MESH_AUTO_MODEL_ID)
                .to_owned(),
        ),
        None => mesh_preset_env_model_id(&record.env_vars),
    }
}

fn resolve_definition_less(
    record: &ManagedAgentRecord,
    global: &GlobalAgentConfig,
) -> Result<EffectiveAgentConfig, String> {
    let model = match non_blank(record.model.as_deref()) {
        Some(m) => ResolvedField {
            value: Some(m.to_owned()),
            source: ConfigSource::InstanceLegacy,
        },
        None => ResolvedField {
            value: global.model.clone(),
            source: ConfigSource::Global,
        },
    };

    let provider = match non_blank(record.provider.as_deref()) {
        Some(p) => ResolvedField {
            value: Some(p.to_owned()),
            source: ConfigSource::InstanceLegacy,
        },
        None => ResolvedField {
            value: global.provider.clone(),
            source: ConfigSource::Global,
        },
    };

    let base_system_prompt = ResolvedField {
        value: non_blank(record.system_prompt.as_deref()).map(str::to_owned),
        source: ConfigSource::InstanceLegacy,
    };

    let capability_policy = resolve_effective_capability_policy(record, &[], global);
    let system_prompt =
        compose_final_prompt(base_system_prompt.clone(), &capability_policy.policy)?;

    let mut config = EffectiveAgentConfig {
        model,
        provider,
        system_prompt,
        base_system_prompt,
        capability_policy: capability_policy.policy,
    };

    // Legacy mesh compatibility. A record with an explicit `provider` has
    // already stated its intent — including switching AWAY from mesh, which
    // leaves the old marker and env bytes behind — so its legacy bytes are
    // never consulted. Only a record that never carried a provider at all
    // falls back, and both fields move together so the single mesh gate
    // (`relay_mesh_model_id`) and the spawned model agree.
    if non_blank(record.provider.as_deref()).is_none() {
        if let Some(model_ref) = legacy_record_mesh_model_id(record) {
            config.provider = ResolvedField {
                value: Some(RELAY_MESH_PROVIDER_ID.to_owned()),
                source: ConfigSource::InstanceLegacy,
            };
            config.model = ResolvedField {
                value: Some(model_ref),
                source: ConfigSource::InstanceLegacy,
            };
        }
    }

    Ok(config)
}

pub fn resolve_effective_config(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    global: &GlobalAgentConfig,
) -> EffectiveConfigResult {
    match &record.persona_id {
        Some(pid) => match definitions.iter().find(|d| d.id == *pid) {
            Some(def) => match resolve_linked(record, def, global) {
                Ok(config) => EffectiveConfigResult::Resolved(config),
                Err(error) => EffectiveConfigResult::InvalidPolicy { error },
            },
            None => EffectiveConfigResult::OrphanedInstance {
                record_pubkey: record.pubkey.clone(),
                missing_persona_id: pid.clone(),
            },
        },
        None => match resolve_definition_less(record, global) {
            Ok(config) => EffectiveConfigResult::Resolved(config),
            Err(error) => EffectiveConfigResult::InvalidPolicy { error },
        },
    }
}

pub fn resolve_effective_model_provider_pair(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    global: &GlobalAgentConfig,
) -> Option<(Option<String>, Option<String>)> {
    match resolve_effective_config(record, definitions, global) {
        EffectiveConfigResult::Resolved(cfg) => Some((cfg.model.value, cfg.provider.value)),
        EffectiveConfigResult::OrphanedInstance { .. }
        | EffectiveConfigResult::InvalidPolicy { .. } => None,
    }
}

/// Save-time guard (SPEC-004): the prospective COMPOSED prompt — the
/// effective base prompt (live definition prompt for linked instances, the
/// record's own prompt for legacy ones) plus the resolved policy's skill
/// sections — must fit the 128 KiB cap on every managed-agent save path,
/// including edits that touch neither the prompt nor the policy (a stored
/// `Selected`-skills policy still composes against the new base). Spawn and
/// deploy compose at resolve time and refuse over-cap prompts with
/// `InvalidPolicy`; this surfaces the same failure at the save boundary
/// (plan §07 row 5), using the production resolver so the two can never
/// drift.
///
/// Orphaned linked records pass — spawn/deploy refuse them via
/// `require_resolved` with the shared orphan error.
pub fn validate_effective_composed_prompt(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    global: &GlobalAgentConfig,
) -> Result<(), String> {
    match resolve_effective_config(record, definitions, global) {
        EffectiveConfigResult::Resolved(_) | EffectiveConfigResult::OrphanedInstance { .. } => {
            Ok(())
        }
        EffectiveConfigResult::InvalidPolicy { error } => Err(error),
    }
}

/// Inbound-wire counterpart of [`validate_effective_composed_prompt`]
/// (round2-general-002, kind:30177 half), called from
/// `apply_inbound_managed_agent` AFTER the projected override is patched
/// onto the local record. The override composes against the LOCAL base
/// prompt (live definition prompt for linked instances, the record's own
/// for legacy ones), so a cross-client event could otherwise persist an
/// over-cap override every local save path rejects — wedging spawn/deploy
/// at resolve time until manual repair.
///
/// Tolerance-shaped like the 30175 parse boundary: never reject the event,
/// never skip the apply (retention dedup would poison the head). Only the
/// SKILLS sub-group affects composed size, so an over-cap override drops
/// skills to Inherit; when that leaves an all-default override the record
/// stores `None` (inherit the definition) — `Some(default)` would silently
/// mask a non-default definition policy. Gated on the cap error only: an
/// unknown skill id (hand-edited store) keeps its loud failure.
pub fn downgrade_over_cap_inbound_override(
    record: &mut ManagedAgentRecord,
    definitions: &[AgentDefinition],
    global: &GlobalAgentConfig,
) {
    let Some(policy) = &record.capability_policy_override else {
        return;
    };
    if !matches!(policy.skills, SkillPolicy::Selected { .. }) {
        return;
    }
    let over_cap = match resolve_effective_config(record, definitions, global) {
        EffectiveConfigResult::InvalidPolicy { error } => {
            error.starts_with(COMPOSED_CAP_ERROR_PREFIX)
        }
        _ => false,
    };
    if !over_cap {
        return;
    }
    let Some(mut policy) = record.capability_policy_override.take() else {
        return;
    };
    policy.skills = SkillPolicy::Inherit;
    record.capability_policy_override = if policy.is_default() {
        None
    } else {
        Some(policy)
    };
}

/// The relay-mesh preflight decision for `record`, resolved the same way
/// spawn resolves its mesh env: through `resolve_effective_config` (which
/// folds in the definition → global fallback). A linked instance's own
/// `provider`/`model`/`relay_mesh` bytes never contribute; a definition-less
/// legacy record may fall back to them via `legacy_record_mesh_model_id`,
/// which is confined to `resolve_definition_less`.
///
/// `None` covers both "not a mesh agent" and "orphaned instance" — an orphan
/// never spawns (see `require_resolved`), so it never needs a mesh preflight
/// either; the caller's own orphan handling downstream is unaffected, this
/// just avoids tripping mesh bootstrap for a start that will be refused.
pub fn resolve_effective_relay_mesh_model_id(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    global: &GlobalAgentConfig,
) -> Option<String> {
    match resolve_effective_config(record, definitions, global) {
        EffectiveConfigResult::Resolved(cfg) => cfg.relay_mesh_model_id(),
        EffectiveConfigResult::OrphanedInstance { .. }
        | EffectiveConfigResult::InvalidPolicy { .. } => None,
    }
}

/// The single user-facing message for a linked instance whose definition no
/// longer exists. Shared by every path that must refuse to act on an orphan:
/// the spawn boundary (`spawn_agent_child`), the interactive start command,
/// and provider deploy.
pub const ORPHANED_INSTANCE_ERROR: &str =
    "This agent's configuration is missing — it may still be \
     syncing or was deleted on another device.";

impl EffectiveConfigResult {
    /// Unwrap into the resolved config, or the shared orphan-refusal error.
    pub fn require_resolved(self) -> Result<EffectiveAgentConfig, String> {
        match self {
            EffectiveConfigResult::Resolved(cfg) => Ok(cfg),
            EffectiveConfigResult::OrphanedInstance { .. } => {
                Err(ORPHANED_INSTANCE_ERROR.to_string())
            }
            EffectiveConfigResult::InvalidPolicy { error } => Err(error),
        }
    }
}

#[cfg(test)]
mod capability_tests;
#[cfg(test)]
mod tests;
