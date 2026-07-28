//! Buzz prompt skills: a static catalog of prompt-text sections delivered
//! through the composed system prompt.
//!
//! A prompt skill is *content*, not a harness mechanism: selecting one appends
//! a deterministic `[Skill: …]` section to the agent's composed system prompt
//! (see `effective_config`), so skill delivery works identically on local
//! spawn, the restart hash, and provider deploy with zero divergence risk.
//!
//! v1 seeds exactly ONE honest skill — `buzz-cli`, reusing the same
//! `nest_skill.md` asset the nest initializer writes to
//! `.agents/skills/buzz-cli/SKILL.md`. Do not add product skills here without
//! a reviewed prompt; unit tests inject fixture skills via
//! [`compose_skill_sections_from`] instead.

use super::types::BuzzSkillId;

/// One catalog entry. `prompt` is the verbatim section body.
pub struct BuzzPromptSkill {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub prompt: &'static str,
}

/// Per-skill prompt byte cap (catalog-enforced; static assert in tests).
/// Follows the env-var precedent `MAX_ENV_VALUE_BYTES` (32 KiB).
pub(crate) const MAX_SKILL_BYTES: usize = 32 * 1024;
/// Combined skill-text byte cap across all selected skills.
pub(crate) const MAX_SKILLS_TOTAL_BYTES: usize = 64 * 1024;
/// Composed final prompt byte cap (persona prompt + skill sections).
pub(crate) const MAX_COMPOSED_PROMPT_BYTES: usize = 128 * 1024;

/// The static catalog. Ids are the `BuzzSkillId` wire values.
pub(crate) const BUZZ_PROMPT_SKILLS: &[BuzzPromptSkill] = &[BuzzPromptSkill {
    id: "buzz-cli",
    label: "Buzz CLI",
    description: "How to use the `buzz` CLI for channels, messages, and agent operations.",
    prompt: include_str!("nest_skill.md"),
}];

/// Frontend-facing skill metadata (prompt text deliberately NOT sent).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuzzPromptSkillInfo {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Catalog projection for the `list_buzz_prompt_skills` IPC command.
pub(crate) fn buzz_prompt_skill_infos() -> Vec<BuzzPromptSkillInfo> {
    BUZZ_PROMPT_SKILLS
        .iter()
        .map(|skill| BuzzPromptSkillInfo {
            id: skill.id.to_string(),
            label: skill.label.to_string(),
            description: skill.description.to_string(),
        })
        .collect()
}

/// Validate a selected-skill id list against the static catalog and the size
/// caps. Unknown ids are rejected by name; the combined cap error names the
/// offending skills and the combined size (plan §07 row 5).
pub(crate) fn validate_skill_selection(ids: &[BuzzSkillId]) -> Result<(), String> {
    validate_skill_selection_from(BUZZ_PROMPT_SKILLS, ids)
}

/// Whether `id` exists in the static catalog. Used at the inbound wire
/// boundary to filter unknown future skill ids (general-005) — unknown ids
/// are dropped there, never stored, so this never drives a save-time error.
pub(crate) fn is_known_skill_id(id: &str) -> bool {
    BUZZ_PROMPT_SKILLS.iter().any(|skill| skill.id == id)
}

/// Catalog-injectable half of [`validate_skill_selection`] so tests can pin
/// the size rules with fixture skills.
///
/// Duplicate ids are deduplicated BEFORE the combined cap is enforced
/// (SPEC-R2-003): composition sorts + dedupes (the §1.1 dedupe-preserving
/// contract), so counting each occurrence would falsely reject a selection
/// whose deduplicated content fits — and the tolerant wire boundary already
/// dedupes before validating, so the local boundary must agree.
pub(crate) fn validate_skill_selection_from(
    catalog: &[BuzzPromptSkill],
    ids: &[BuzzSkillId],
) -> Result<(), String> {
    let mut combined = 0usize;
    let mut known: Vec<(&str, usize)> = Vec::with_capacity(ids.len());
    for id in ids {
        let skill = catalog
            .iter()
            .find(|skill| skill.id == id)
            .ok_or_else(|| format!("unknown Buzz skill id '{id}'"))?;
        if skill.prompt.len() > MAX_SKILL_BYTES {
            return Err(format!(
                "skill '{}' exceeds the per-skill limit ({} > {} bytes)",
                skill.id,
                skill.prompt.len(),
                MAX_SKILL_BYTES
            ));
        }
        if known.iter().any(|(known_id, _)| *known_id == skill.id) {
            continue;
        }
        combined += skill.prompt.len();
        known.push((skill.id, skill.prompt.len()));
    }
    if combined > MAX_SKILLS_TOTAL_BYTES {
        let names = known
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "selected skills ({names}) exceed the combined skill limit ({combined} > {} bytes)",
            MAX_SKILLS_TOTAL_BYTES
        ));
    }
    Ok(())
}

/// Render the skill sections appended to the persona prompt. Deterministic:
/// selected ids are sorted ascending (deduped) and each renders as
/// `"\n\n[Skill: {label}]\n{prompt}"`. The header keeps the base/persona
/// boundary recoverable (same precedent as `framed_system_prompt` in
/// buzz-acp).
///
/// Unknown ids fail loudly — save-time validation already rejects them, so an
/// unknown id here means a hand-edited store, which must not silently drop
/// policy. The combined cap is re-checked as defense in depth.
pub(crate) fn compose_skill_sections(ids: &[BuzzSkillId]) -> Result<String, String> {
    compose_skill_sections_from(BUZZ_PROMPT_SKILLS, ids)
}

/// Catalog-injectable half of [`compose_skill_sections`] for unit tests.
pub(crate) fn compose_skill_sections_from(
    catalog: &[BuzzPromptSkill],
    ids: &[BuzzSkillId],
) -> Result<String, String> {
    validate_skill_selection_from(catalog, ids)?;

    let mut sorted: Vec<&BuzzPromptSkill> = ids
        .iter()
        .map(|id| {
            catalog
                .iter()
                .find(|skill| skill.id == id)
                .expect("validated above")
        })
        .collect();
    sorted.sort_by(|a, b| a.id.cmp(b.id));
    sorted.dedup_by(|a, b| a.id == b.id);

    let mut out = String::new();
    for skill in sorted {
        out.push_str("\n\n[Skill: ");
        out.push_str(skill.label);
        out.push_str("]\n");
        out.push_str(skill.prompt);
    }
    Ok(out)
}

/// Byte length of the deterministic section rendering for `ids` (sorted +
/// deduped) WITHOUT building the string — the composed-cap check at the
/// inbound wire boundary (kind:30175) must measure before deciding to drop
/// an over-cap skills sub-group (round2-general-002). Unknown ids contribute
/// nothing: the tolerant parse boundary filters them before this runs, so
/// they can never reach the store.
pub(crate) fn skill_sections_len(ids: &[BuzzSkillId]) -> usize {
    skill_sections_len_from(BUZZ_PROMPT_SKILLS, ids)
}

/// Catalog-injectable half of [`skill_sections_len`].
pub(crate) fn skill_sections_len_from(catalog: &[BuzzPromptSkill], ids: &[BuzzSkillId]) -> usize {
    let mut sorted: Vec<&BuzzPromptSkill> = ids
        .iter()
        .filter_map(|id| catalog.iter().find(|skill| skill.id == id))
        .collect();
    sorted.sort_by(|a, b| a.id.cmp(b.id));
    sorted.dedup_by(|a, b| a.id == b.id);
    sorted
        .iter()
        .map(|skill| {
            // Mirrors the exact compose rendering: "\n\n[Skill: " + label +
            // "]\n" + prompt.
            "\n\n[Skill: ".len() + skill.label.len() + "]\n".len() + skill.prompt.len()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[BuzzPromptSkill] = &[
        BuzzPromptSkill {
            id: "alpha",
            label: "Alpha",
            description: "first",
            prompt: "alpha body",
        },
        BuzzPromptSkill {
            id: "zeta",
            label: "Zeta",
            description: "second",
            prompt: "zeta body",
        },
    ];

    #[test]
    fn catalog_prompts_fit_per_skill_cap() {
        for skill in BUZZ_PROMPT_SKILLS {
            assert!(
                skill.prompt.len() <= MAX_SKILL_BYTES,
                "catalog skill '{}' exceeds {} bytes",
                skill.id,
                MAX_SKILL_BYTES
            );
        }
    }

    #[test]
    fn catalog_ids_are_unique_and_nonempty() {
        let mut ids: Vec<&str> = BUZZ_PROMPT_SKILLS.iter().map(|s| s.id).collect();
        assert!(ids.iter().all(|id| !id.is_empty()));
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUZZ_PROMPT_SKILLS.len());
    }

    #[test]
    fn compose_sorts_ids_ascending_with_section_headers() {
        let out = compose_skill_sections_from(FIXTURE, &["zeta".to_string(), "alpha".to_string()])
            .unwrap();
        assert_eq!(
            out,
            "\n\n[Skill: Alpha]\nalpha body\n\n[Skill: Zeta]\nzeta body"
        );
    }

    #[test]
    fn compose_dedupes_repeated_ids() {
        let out = compose_skill_sections_from(FIXTURE, &["alpha".to_string(), "alpha".to_string()])
            .unwrap();
        assert_eq!(out, "\n\n[Skill: Alpha]\nalpha body");
    }

    #[test]
    fn compose_rejects_unknown_id_by_name() {
        let err = compose_skill_sections_from(FIXTURE, &["nope".to_string()]).unwrap_err();
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn static_catalog_compose_matches_injected_catalog() {
        let via_static = compose_skill_sections(&["buzz-cli".to_string()]).unwrap();
        let via_injected =
            compose_skill_sections_from(BUZZ_PROMPT_SKILLS, &["buzz-cli".to_string()]).unwrap();
        assert_eq!(via_static, via_injected);
        assert!(
            via_static.starts_with("\n\n[Skill: Buzz CLI]\n"),
            "{via_static}"
        );
    }

    #[test]
    fn combined_cap_error_names_skills_and_size() {
        // Each skill fits the per-skill cap; together they exceed the
        // combined cap, so the combined check is what fires.
        let big: &'static str = Box::leak("x".repeat(MAX_SKILL_BYTES).into_boxed_str());
        let catalog = &[
            BuzzPromptSkill {
                id: "big-a",
                label: "Big A",
                description: "",
                prompt: big,
            },
            BuzzPromptSkill {
                id: "big-b",
                label: "Big B",
                description: "",
                prompt: big,
            },
            BuzzPromptSkill {
                id: "big-c",
                label: "Big C",
                description: "",
                prompt: big,
            },
        ];
        let err = validate_skill_selection_from(
            catalog,
            &[
                "big-a".to_string(),
                "big-b".to_string(),
                "big-c".to_string(),
            ],
        )
        .unwrap_err();
        assert!(err.contains("big-a"), "{err}");
        assert!(err.contains("big-c"), "{err}");
        assert!(err.contains(&(3 * MAX_SKILL_BYTES).to_string()), "{err}");
    }

    #[test]
    fn duplicate_ids_are_deduplicated_before_the_combined_cap() {
        // SPEC-R2-003: 7× buzz-cli is 74,466 bytes counted per-occurrence —
        // over the 64 KiB combined cap — but composition dedupes to ONE
        // section, so validation must dedupe first and accept. The tolerant
        // wire boundary already behaved this way; the local boundaries now
        // agree.
        let ids: Vec<BuzzSkillId> = (0..7).map(|_| "buzz-cli".to_string()).collect();
        validate_skill_selection(&ids).unwrap();
        let composed = compose_skill_sections(&ids).unwrap();
        assert_eq!(
            composed,
            "\n\n[Skill: Buzz CLI]\n".to_string() + BUZZ_PROMPT_SKILLS[0].prompt
        );

        // An unknown id is still named even amid duplicates.
        let mut with_unknown = ids.clone();
        with_unknown.push("nope".to_string());
        let err = validate_skill_selection(&with_unknown).unwrap_err();
        assert!(err.contains("nope"), "{err}");

        // An empty selection is still rejected at the policy layer.
        let err = crate::managed_agents::types::validate_capability_policy(
            &crate::managed_agents::types::AgentCapabilityPolicy {
                skills: crate::managed_agents::types::SkillPolicy::Selected { selected: vec![] },
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("at least one skill"), "{err}");

        // Duplicates that stay over the cap AFTER deduping still fail.
        let big: &'static str = Box::leak("x".repeat(MAX_SKILL_BYTES).into_boxed_str());
        let catalog = &[
            BuzzPromptSkill {
                id: "big-a",
                label: "Big A",
                description: "",
                prompt: big,
            },
            BuzzPromptSkill {
                id: "big-b",
                label: "Big B",
                description: "",
                prompt: big,
            },
            BuzzPromptSkill {
                id: "big-c",
                label: "Big C",
                description: "",
                prompt: big,
            },
        ];
        let err = validate_skill_selection_from(
            catalog,
            &[
                "big-a".to_string(),
                "big-a".to_string(),
                "big-b".to_string(),
                "big-b".to_string(),
                "big-c".to_string(),
            ],
        )
        .unwrap_err();
        assert!(err.contains(&(3 * MAX_SKILL_BYTES).to_string()), "{err}");
    }

    #[test]
    fn section_len_matches_the_composed_rendering() {
        // The no-materialize length helper feeds the inbound composed-cap
        // check — it must agree with compose byte-for-byte or the wire
        // boundary would accept/reject a different set than spawn does.
        for ids in [
            vec!["buzz-cli".to_string()],
            vec!["buzz-cli".to_string(), "buzz-cli".to_string()],
            vec![],
        ] {
            assert_eq!(
                skill_sections_len(&ids),
                compose_skill_sections(&ids).unwrap().len(),
                "ids: {ids:?}"
            );
        }
        let fixture_ids = vec!["zeta".to_string(), "alpha".to_string(), "alpha".to_string()];
        assert_eq!(
            skill_sections_len_from(FIXTURE, &fixture_ids),
            compose_skill_sections_from(FIXTURE, &fixture_ids)
                .unwrap()
                .len()
        );
        // Unknown ids contribute nothing (the wire filter drops them first).
        assert_eq!(skill_sections_len(&["nope".to_string()]), 0);
    }
}
