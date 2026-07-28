//! Harness-neutral capability policy types, split from `types.rs`
//! (file-size cap). Serde contracts and validation live here so the store,
//! wire projections, and IPC requests share one definition.

use serde::{Deserialize, Serialize};

// ── Harness-neutral capability policy ────────────────────────────────────────
//
// Semantic, portable intent ("which tool capabilities may this agent use,
// which Buzz prompt skills does it carry") stored on definitions
// (`AgentDefinition.capability_policy`) and instances
// (`ManagedAgentRecord.capability_policy_override`). Harness-specific delivery
// (CLI flags where a launch-tested transport exists; prompt sections for
// skills) is compiled at the descriptor seam from this neutral form — never
// persisted.
//
// Absent-stable serialization is LOAD-BEARING: `AgentDefinition` is published
// as kind:30175 and `persona_content_hash` is the fleet-wide drift basis, so a
// default policy must serialize byte-identically to a store written before the
// feature existed (guarded by `policy_absent_*` tests, mirroring the
// behavioral-quad activation).

/// Semantic, harness-neutral tool capability. Wire names are stable dotted
/// strings; per-variant renames because dots are not a serde casing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ToolCapabilityId {
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "code.search")]
    CodeSearch,
    #[serde(rename = "code.intelligence")]
    CodeIntelligence,
    #[serde(rename = "shell.execute")]
    ShellExecute,
    #[serde(rename = "browser")]
    Browser,
    #[serde(rename = "web.search")]
    WebSearch,
    #[serde(rename = "subagents")]
    Subagents,
    #[serde(rename = "task.tracking")]
    TaskTracking,
    #[serde(rename = "image.inspect")]
    ImageInspect,
}

impl ToolCapabilityId {
    /// Every capability, in declaration order (the canonical listing order).
    pub const ALL: [ToolCapabilityId; 10] = [
        Self::FilesRead,
        Self::FilesWrite,
        Self::CodeSearch,
        Self::CodeIntelligence,
        Self::ShellExecute,
        Self::Browser,
        Self::WebSearch,
        Self::Subagents,
        Self::TaskTracking,
        Self::ImageInspect,
    ];

    /// Stable dotted wire id (e.g. `"files.read"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FilesRead => "files.read",
            Self::FilesWrite => "files.write",
            Self::CodeSearch => "code.search",
            Self::CodeIntelligence => "code.intelligence",
            Self::ShellExecute => "shell.execute",
            Self::Browser => "browser",
            Self::WebSearch => "web.search",
            Self::Subagents => "subagents",
            Self::TaskTracking => "task.tracking",
            Self::ImageInspect => "image.inspect",
        }
    }

    /// Inverse of [`Self::as_str`], used ONLY at the inbound wire boundary
    /// (kind:30175/30177) to filter unknown future ids before constructing
    /// the stored closed enum — never a serde catch-all (general-005).
    pub fn from_wire_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.as_str() == id)
    }
}

/// Tool capability policy for an agent. `HarnessDefault` (absent) keeps the
/// harness's ambient tool set — byte-identical behavior to pre-feature stores.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ToolPolicy {
    #[default]
    HarnessDefault,
    None,
    Selected {
        selected: Vec<ToolCapabilityId>,
    },
}

impl ToolPolicy {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::HarnessDefault)
    }
}

/// A Buzz prompt-skill id. Validated against the static catalog
/// (`prompt_skills::BUZZ_PROMPT_SKILLS`) at every boundary.
pub type BuzzSkillId = String;

/// Skill policy for an agent. `Inherit` (absent) keeps harness-default
/// behavior: ambient native skills untouched, no Buzz skill sections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SkillPolicy {
    /// Harness-default behavior: ambient native skills untouched, no Buzz skill sections.
    #[default]
    Inherit,
    None,
    Selected {
        selected: Vec<BuzzSkillId>,
    },
}

impl SkillPolicy {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

/// The portable capability policy group. Travels as one unit: an update
/// replaces the whole group (validated), never individual modes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityPolicy {
    #[serde(default, skip_serializing_if = "ToolPolicy::is_default")]
    pub tools: ToolPolicy,
    #[serde(default, skip_serializing_if = "SkillPolicy::is_default")]
    pub skills: SkillPolicy,
}

impl AgentCapabilityPolicy {
    pub fn is_default(&self) -> bool {
        self.tools.is_default() && self.skills.is_default()
    }
}

/// Validate a capability policy at a save/apply boundary.
///
/// Rules:
/// - `Selected` with an empty vec → Err ("selected mode requires at least one …").
/// - Unknown skill ids → Err naming the id. Tool ids are an enum, so
///   deserialization already fails closed on unknown ids.
/// - Selected skill text must fit the byte caps (per-skill ≤ 32 KiB is a
///   static catalog assert; combined skill text ≤ 64 KiB is checked here and
///   re-checked at compose time as defense in depth).
///
/// Dedupe-preserving-order is the caller's job (see
/// [`normalize_capability_policy`]), same convention as allowlists.
pub fn validate_capability_policy(policy: &AgentCapabilityPolicy) -> Result<(), String> {
    if let ToolPolicy::Selected { selected } = &policy.tools {
        if selected.is_empty() {
            return Err("tools mode 'selected' requires at least one capability".to_string());
        }
    }
    if let SkillPolicy::Selected { selected } = &policy.skills {
        if selected.is_empty() {
            return Err("skills mode 'selected' requires at least one skill".to_string());
        }
        crate::managed_agents::prompt_skills::validate_skill_selection(selected)?;
    }
    Ok(())
}

/// Normalize a policy in place: dedupe `selected` vecs preserving first-seen
/// order (same convention as `validate_respond_to_allowlist`). Run AFTER
/// [`validate_capability_policy`] so an empty selection is still rejected.
pub fn normalize_capability_policy(policy: &mut AgentCapabilityPolicy) {
    fn dedupe<T: PartialEq>(values: &mut Vec<T>) {
        let mut out: Vec<T> = Vec::with_capacity(values.len());
        for value in std::mem::take(values) {
            if !out.contains(&value) {
                out.push(value);
            }
        }
        *values = out;
    }
    if let ToolPolicy::Selected { selected } = &mut policy.tools {
        dedupe(selected);
    }
    if let SkillPolicy::Selected { selected } = &mut policy.skills {
        dedupe(selected);
    }
}

/// Whether a runtime's capability transport is verified for explicit policy.
/// Serializes snake_case so the TypeScript consumer can switch on it.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupportLevel {
    /// Launch-tested tool mappings + flags. No runtime ships this in v1 —
    /// omp was downgraded after HC-001 (`docs/hc001-omp-capability-transport.md`)
    /// proved its flag surface does not enforce the selected set in `acp`
    /// mode. The level stays for runtimes that land with launch-test evidence.
    Verified,
    /// Harness-managed capabilities — no explicit tool policy allowed; the
    /// harness owns its tool set. Skills still deliver via prompt sections.
    HarnessManaged,
}

/// The capability-policy facts the UI needs, projected from
/// `KnownAcpRuntime`'s transport metadata at catalog-build time. The frontend
/// never maintains a rival copy of this table (AGENTS.md one rule).
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeCapabilitySupport {
    pub tool_policy: CapabilitySupportLevel,
    pub supported_tool_ids: Vec<ToolCapabilityId>,
    pub unsupported_tool_ids: Vec<ToolCapabilityId>,
    pub skills_disable: bool,
    /// Shown beside the skills controls when the runtime CAN disable its
    /// ambient skills (e.g. a runtime whose disable flag also drops the
    /// bundled buzz-cli skill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_skill_note: Option<String>,
}

impl RuntimeCapabilitySupport {
    /// Preset/custom entries: harness-managed capabilities, every semantic id
    /// unsupported for explicit policy (v1).
    pub fn harness_managed() -> Self {
        Self {
            tool_policy: CapabilitySupportLevel::HarnessManaged,
            supported_tool_ids: Vec::new(),
            unsupported_tool_ids: ToolCapabilityId::ALL.to_vec(),
            skills_disable: false,
            ambient_skill_note: None,
        }
    }
}

// ── Inbound wire tolerance (kind:30175/30177 parse boundaries only) ────────
//
// NIP-AP requires readers to "ignore unknown ids and unknown sub-groups"
// (capability_policy row), so a newer client's policy must never make an
// older client reject the WHOLE event. The tolerant DTO below is used ONLY
// in `deserialize_with` at the two event-content parse boundaries:
//
// - unknown tool AND skill ids are filtered out before the stored closed
//   enum is constructed (the storage enum's derive is NOT loosened);
// - an empty-after-filter or unknown-mode sub-group drops to its default
//   (`HarnessDefault`/`Inherit` — the protocol's safe state);
// - a structurally malformed policy group drops to the default policy;
// - the result is validated and normalized, so what lands in the store is
//   always a value the save boundary itself would have accepted.
//
// Writers never touch this path: the projections keep serializing the
// closed-enum structs, so re-publish emits known ids only, absent-stable
// serde and `persona_content_hash` inputs are byte-identical, and field
// order is unchanged.

/// One tolerant policy sub-group: `mode` as a free string plus string ids.
#[derive(Debug, Deserialize)]
struct WirePolicyGroup {
    mode: String,
    #[serde(default)]
    selected: Vec<String>,
}

/// Tolerant mirror of [`AgentCapabilityPolicy`]: sub-groups as raw JSON so a
/// malformed one drops in isolation instead of failing the policy parse.
#[derive(Debug, Deserialize)]
struct WireCapabilityPolicy {
    #[serde(default)]
    tools: Option<serde_json::Value>,
    #[serde(default)]
    skills: Option<serde_json::Value>,
}

fn filter_tool_group(group: Option<serde_json::Value>) -> ToolPolicy {
    let Some(group) = group else {
        return ToolPolicy::HarnessDefault;
    };
    let Ok(group) = serde_json::from_value::<WirePolicyGroup>(group) else {
        return ToolPolicy::HarnessDefault;
    };
    match group.mode.as_str() {
        "none" => ToolPolicy::None,
        "selected" => {
            let known: Vec<ToolCapabilityId> = group
                .selected
                .iter()
                .filter_map(|id| ToolCapabilityId::from_wire_id(id))
                .collect();
            if known.is_empty() {
                ToolPolicy::HarnessDefault
            } else {
                ToolPolicy::Selected { selected: known }
            }
        }
        // "harness_default" and unknown future modes: the safe state.
        _ => ToolPolicy::HarnessDefault,
    }
}

fn filter_skill_group(group: Option<serde_json::Value>) -> SkillPolicy {
    let Some(group) = group else {
        return SkillPolicy::Inherit;
    };
    let Ok(group) = serde_json::from_value::<WirePolicyGroup>(group) else {
        return SkillPolicy::Inherit;
    };
    match group.mode.as_str() {
        "none" => SkillPolicy::None,
        "selected" => {
            let known: Vec<BuzzSkillId> = group
                .selected
                .into_iter()
                .filter(|id| crate::managed_agents::prompt_skills::is_known_skill_id(id))
                .collect();
            if known.is_empty() {
                SkillPolicy::Inherit
            } else {
                SkillPolicy::Selected { selected: known }
            }
        }
        _ => SkillPolicy::Inherit,
    }
}

/// Filter a raw wire value into the stored closed-enum policy (see the module
/// comment above for the exact semantics). Total: never fails, never carries
/// an unknown id.
pub(crate) fn capability_policy_from_wire_value(
    value: &serde_json::Value,
) -> AgentCapabilityPolicy {
    let Ok(dto) = serde_json::from_value::<WireCapabilityPolicy>(value.clone()) else {
        return AgentCapabilityPolicy::default();
    };
    let mut policy = AgentCapabilityPolicy {
        tools: filter_tool_group(dto.tools),
        skills: filter_skill_group(dto.skills),
    };
    normalize_capability_policy(&mut policy);
    if validate_capability_policy(&policy).is_err() {
        // Unreachable post-filter (ids are known and selections non-empty);
        // the protocol's safe state if that ever changes.
        return AgentCapabilityPolicy::default();
    }
    policy
}

/// `deserialize_with` for `AgentCapabilityPolicy` fields on event-content
/// structs (kind:30175). Absent keys still hit `#[serde(default)]`; present
/// values are filtered through the tolerant boundary.
pub(crate) fn deserialize_capability_policy_tolerant<'de, D>(
    deserializer: D,
) -> Result<AgentCapabilityPolicy, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(capability_policy_from_wire_value(&value))
}

/// `deserialize_with` for `Option<AgentCapabilityPolicy>` fields on
/// event-content structs (kind:30177's instance override).
pub(crate) fn deserialize_opt_capability_policy_tolerant<'de, D>(
    deserializer: D,
) -> Result<Option<AgentCapabilityPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Null => None,
        other => Some(capability_policy_from_wire_value(&other)),
    })
}
