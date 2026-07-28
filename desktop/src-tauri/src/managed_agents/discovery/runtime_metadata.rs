/// Static capabilities and installation metadata for a known ACP runtime.
/// `Copy` so tests can derive fixture transports from a real entry via
/// struct-update syntax.
#[derive(Clone, Copy)]
pub(crate) struct KnownAcpRuntime {
    pub id: &'static str,
    pub label: &'static str,
    pub commands: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub avatar_url: &'static str,
    /// Legacy MCP server binary field. Vestigial — all agents now use the bundled CLI
    /// directly. Will be removed when runtime discovery is simplified.
    pub mcp_command: Option<&'static str>,
    /// Whether to enable MCP hook tools (`_Stop`, `_PostCompact`) for this agent.
    pub mcp_hooks: bool,
    /// CLI binary that indicates partial install (e.g. `"claude"` when `claude-agent-acp` is missing).
    pub underlying_cli: Option<&'static str>,
    /// Shell commands to install the runtime CLI itself (run sequentially).
    pub cli_install_commands: &'static [&'static str],
    /// Windows-specific CLI install commands (e.g. PowerShell installers).
    /// When non-empty on Windows, these are used instead of `cli_install_commands`.
    #[allow(dead_code)] // read only on Windows via cli_install_commands_for_os()
    pub cli_install_commands_windows: &'static [&'static str],
    /// Shell commands to install the ACP adapter (run sequentially, after CLI).
    pub adapter_install_commands: &'static [&'static str],
    /// Official CLI installation documentation.
    pub cli_install_instructions_url: &'static str,
    /// ACP adapter installation documentation.
    pub adapter_install_instructions_url: &'static str,
    /// Human-readable hint about installing the CLI binary.
    pub cli_install_hint: &'static str,
    /// Human-readable hint about installing the ACP adapter.
    pub adapter_install_hint: &'static str,
    /// Harness-specific skill discovery directory (e.g. `.goose/skills`).
    /// `Some(dir)` → Buzz creates a symlink at `<nest>/<dir>/buzz-cli`
    /// pointing to the canonical `.agents/skills/buzz-cli`. `None` → this
    /// runtime reads the canonical path directly or has no skill support.
    pub skill_dir: Option<&'static str>,
    /// Whether this runtime handles model switching via ACP protocol natively.
    /// Currently unused — env var injection runs unconditionally regardless of
    /// this value. Retained as scaffolding for when ACP model switching matures.
    #[allow(dead_code)]
    pub supports_acp_model_switching: bool,
    pub model_env_var: Option<&'static str>,
    pub provider_env_var: Option<&'static str>,
    pub provider_locked: bool,
    pub default_env: &'static [(&'static str, &'static str)],
    pub config_file_path: Option<&'static str>,
    #[allow(dead_code)] // reserved for format-based dispatch when readers are unified
    pub config_file_format: Option<&'static str>,
    pub supports_acp_native_config: bool, // tier 1a: config/read+write
    pub thinking_env_var: Option<&'static str>,
    /// Env var for normalizing `max_output_tokens`. `None` when the harness
    /// does not have a first-class env var for this field (config-file only).
    pub max_tokens_env_var: Option<&'static str>,
    /// Env var for normalizing `context_limit`. `None` when not applicable.
    pub context_limit_env_var: Option<&'static str>,
    /// Normalized field keys that must be set for this harness to function.
    /// Used by the config bridge to mark fields as required in the UI.
    /// Keys match the camelCase names used in `NormalizedConfig` (e.g. "model", "provider").
    pub required_normalized_fields: &'static [&'static str],
    /// Human-readable hint shown in Doctor when the runtime is available but not
    /// authenticated. `None` for runtimes that have no login step (goose, buzz-agent).
    pub login_hint: Option<&'static str>,
    /// CLI args for probing authentication status. `args[0]` is the binary name;
    /// the remainder are the subcommand. `None` for runtimes with no login step.
    pub auth_probe_args: Option<&'static [&'static str]>,
    /// Whether the runtime takes a user-selected LLM provider (drives the
    /// provider picker). Replaces the former TS-side
    /// `runtimeSupportsLlmProviderSelection` lookup table (AGENTS.md one rule:
    /// capability facts live ONLY in this Rust catalog).
    pub provider_selection: bool,
    /// Capability-policy transport: semantic tool mappings + the CLI flags a
    /// structured policy compiles to. `CapabilityTransport::default()` (empty
    /// mappings, no flags) means harness-managed capabilities — no explicit
    /// tool policy is allowed; skills still deliver via prompt sections.
    pub capability_transport: CapabilityTransport,
}

/// One semantic capability → the native tool names that realize it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CapabilityToolMapping {
    pub capability: crate::managed_agents::types::ToolCapabilityId,
    pub native_tools: &'static [&'static str],
}

/// Where compiled capability flags are spliced into the final args vector.
/// v1 is always `Leading`: omp's global flags (`--tools`, `--no-skills`) must
/// precede the `acp` subcommand token — anything after it is silently ignored
/// by omp's lenient parse (false capability parity, plan §0.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CapabilityFlagPlacement {
    #[default]
    Leading,
}

/// The harness-specific delivery contract for a structured capability policy.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CapabilityTransport {
    /// Verified semantic→native mappings. Empty = harness-managed capabilities
    /// (no explicit tool policy allowed).
    pub tool_mappings: &'static [CapabilityToolMapping],
    /// Flag template for a selected tools policy, joined as `--tools=<csv>`.
    pub tools_select_flag: Option<&'static str>,
    /// Flag for tools.mode=none, e.g. `--no-tools`.
    pub tools_none_flag: Option<&'static str>,
    /// Flag disabling ambient native skills, e.g. `--no-skills`. `None` = the
    /// ambient-skill limitation is surfaced in the UI instead of failing.
    pub skills_disable_flag: Option<&'static str>,
    /// v1: always Leading (flags inserted before the subcommand token).
    pub flag_placement: CapabilityFlagPlacement,
    /// Disclosure shown when skills disable is available, e.g. a runtime
    /// whose disable flag also drops the bundled buzz-cli skill (§0.7).
    pub ambient_skill_note: Option<&'static str>,
}

impl CapabilityTransport {
    /// Harness-managed capabilities: no verified mappings, no flags — explicit
    /// tool policies are rejected; skills still deliver via prompt sections.
    /// An associated const (not just a const fn) because `KnownAcpRuntime`
    /// test fixtures are static-promoted only through const-item paths.
    pub(crate) const HARNESS_MANAGED: Self = Self {
        tool_mappings: &[],
        tools_select_flag: None,
        tools_none_flag: None,
        skills_disable_flag: None,
        flag_placement: CapabilityFlagPlacement::Leading,
        ambient_skill_note: None,
    };

    /// Harness-managed capabilities — see [`Self::HARNESS_MANAGED`].
    pub(crate) const fn harness_managed() -> Self {
        Self::HARNESS_MANAGED
    }
}

impl Default for CapabilityTransport {
    fn default() -> Self {
        Self::harness_managed()
    }
}

impl KnownAcpRuntime {
    /// Whether an explicit tools policy may compile against this runtime's
    /// transport (verified mappings + a select flag).
    pub(crate) fn has_verified_capability_transport(&self) -> bool {
        !self.capability_transport.tool_mappings.is_empty()
            && self.capability_transport.tools_select_flag.is_some()
    }

    /// Project the capability-support facts for the runtime catalog entry —
    /// the ONLY source the UI reads (AGENTS.md one rule).
    pub(crate) fn capability_support(
        &self,
    ) -> crate::managed_agents::types::RuntimeCapabilitySupport {
        use crate::managed_agents::types::{
            CapabilitySupportLevel, RuntimeCapabilitySupport, ToolCapabilityId,
        };
        let supported: Vec<ToolCapabilityId> = self
            .capability_transport
            .tool_mappings
            .iter()
            .map(|mapping| mapping.capability)
            .collect();
        let unsupported: Vec<ToolCapabilityId> = ToolCapabilityId::ALL
            .into_iter()
            .filter(|id| !supported.contains(id))
            .collect();
        RuntimeCapabilitySupport {
            tool_policy: if self.has_verified_capability_transport() {
                CapabilitySupportLevel::Verified
            } else {
                CapabilitySupportLevel::HarnessManaged
            },
            supported_tool_ids: supported,
            unsupported_tool_ids: unsupported,
            skills_disable: self.capability_transport.skills_disable_flag.is_some(),
            ambient_skill_note: self
                .capability_transport
                .ambient_skill_note
                .map(str::to_string),
        }
    }

    /// Return the CLI install commands for the current platform.
    ///
    /// On Windows, returns `cli_install_commands_windows` when non-empty,
    /// falling back to the default `cli_install_commands`. On other platforms
    /// always returns `cli_install_commands`.
    pub fn cli_install_commands_for_os(&self) -> &[&str] {
        #[cfg(windows)]
        {
            if !self.cli_install_commands_windows.is_empty() {
                return self.cli_install_commands_windows;
            }
        }
        self.cli_install_commands
    }
}

/// Harness-managed capabilities (goose, claude, codex, buzz-agent, omp in
/// v1 — omp downgraded by HC-001, see `docs/hc001-omp-capability-transport.md`):
/// no verified mappings, no flags — explicit tool policies are rejected with
/// the unsupported-capability error; skills still deliver via prompt sections.
pub(crate) const HARNESS_MANAGED_TRANSPORT: CapabilityTransport =
    CapabilityTransport::HARNESS_MANAGED;

/// All-zero `KnownAcpRuntime` base for test fixtures: literals struct-update
/// from it and name only the fields their test exercises, so catalog field
/// additions never force an edit in every fixture.
#[cfg(test)]
pub(crate) const EMPTY_RUNTIME: KnownAcpRuntime = KnownAcpRuntime {
    id: "",
    label: "",
    commands: &[],
    aliases: &[],
    avatar_url: "",
    mcp_command: None,
    mcp_hooks: false,
    underlying_cli: None,
    cli_install_commands: &[],
    cli_install_commands_windows: &[],
    adapter_install_commands: &[],
    cli_install_instructions_url: "",
    adapter_install_instructions_url: "",
    cli_install_hint: "",
    adapter_install_hint: "",
    skill_dir: None,
    supports_acp_model_switching: false,
    model_env_var: None,
    provider_env_var: None,
    provider_locked: false,
    default_env: &[],
    config_file_path: None,
    config_file_format: None,
    supports_acp_native_config: false,
    thinking_env_var: None,
    max_tokens_env_var: None,
    context_limit_env_var: None,
    required_normalized_fields: &[],
    login_hint: None,
    auth_probe_args: None,
    provider_selection: false,
    capability_transport: CapabilityTransport::HARNESS_MANAGED,
};

/// omp capability transport is HARNESS-MANAGED in v1 (HC-001, 2026-07-28,
/// `docs/hc001-omp-capability-transport.md`). The launch check against the
/// installed omp 17.1.6 proved the `--tools`/`--no-tools` flag surface does
/// NOT enforce the spec's capability granularity in `omp acp` mode: enabling
/// `read` also unlocks file writes (behavioral: a read-only vector created
/// files), `--no-tools` still admits skill/lesson mutation tools, and the
/// model-visible surface under `--tools=read` includes unrequested devices.
/// Shipping Selected/None tool policies on omp would be false capability
/// parity, so explicit tool policies are rejected exactly like
/// goose/claude/codex/buzz-agent; skills still deliver via prompt sections.
/// ACP turns DO complete under every tested vector (including `--no-tools`),
/// so the §3.1 `always_include` open risk did not materialize — no
/// always-include list. Re-upgrade only with launch-test evidence against a
/// fixed omp release (the doc lists the rerun criteria).
///
/// The mapping table below is what a verified omp transport would compile
/// from; in v1 it serves ONLY as the compiler test-suite's verified-transport
/// fixture (no production runtime references it).
#[cfg(test)]
pub(crate) const VERIFIED_FIXTURE_TOOL_MAPPINGS: &[CapabilityToolMapping] = &[
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::FilesRead,
        native_tools: &["read"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::FilesWrite,
        native_tools: &["edit", "write", "ast_edit"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::CodeSearch,
        native_tools: &["grep", "glob", "ast_grep"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::CodeIntelligence,
        native_tools: &["lsp"],
    },
    // Both execute arbitrary code.
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::ShellExecute,
        native_tools: &["bash", "eval"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::Browser,
        native_tools: &["browser"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::WebSearch,
        native_tools: &["web_search"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::Subagents,
        native_tools: &["task", "hub"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::TaskTracking,
        native_tools: &["todo"],
    },
    CapabilityToolMapping {
        capability: crate::managed_agents::types::ToolCapabilityId::ImageInspect,
        native_tools: &["inspect_image"],
    },
];

/// Verified-transport fixture for compiler/hash tests (see the HC-001 note
/// above): a hypothetical runtime whose flags enforce the selected set.
#[cfg(test)]
pub(crate) const VERIFIED_FIXTURE_TRANSPORT: CapabilityTransport = CapabilityTransport {
    tool_mappings: VERIFIED_FIXTURE_TOOL_MAPPINGS,
    tools_select_flag: Some("--tools"),
    tools_none_flag: Some("--no-tools"),
    skills_disable_flag: Some("--no-skills"),
    flag_placement: CapabilityFlagPlacement::Leading,
    ambient_skill_note: Some("Disabling ambient skills also disables the bundled buzz-cli skill."),
};

/// The built-in ACP runtime catalog. Fork-owned (FORK.md): the omp entry,
/// its capability transport, and the install metadata here are Rudu additions.
pub(crate) const GOOSE_AVATAR_URL: &str = "https://goose-docs.ai/img/logo_dark.png";
pub(crate) const CLAUDE_CODE_AVATAR_URL: &str = "https://anthropic.gallerycdn.vsassets.io/extensions/anthropic/claude-code/2.1.77/1773707456892/Microsoft.VisualStudio.Services.Icons.Default";
pub(crate) const CODEX_AVATAR_URL: &str = "https://openai.gallerycdn.vsassets.io/extensions/openai/chatgpt/26.5313.41514/1773706730621/Microsoft.VisualStudio.Services.Icons.Default";
pub(crate) const BUZZ_AGENT_AVATAR_URL: &str =
    "https://raw.githubusercontent.com/block/buzz/refs/heads/main/crates/buzz-agent/buzz-agent.png";
pub(crate) const OMP_AVATAR_URL: &str =
    "https://raw.githubusercontent.com/can1357/oh-my-pi/main/assets/icon.svg";

pub(crate) const KNOWN_ACP_RUNTIMES: &[KnownAcpRuntime] = &[
    KnownAcpRuntime {
        id: "goose",
        label: "Goose",
        commands: &["goose"],
        aliases: &[],
        avatar_url: GOOSE_AVATAR_URL,
        mcp_command: None,
        mcp_hooks: false,
        underlying_cli: Some("goose"),
        cli_install_commands: &["curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false bash"],
        // Goose's stable release currently publishes only the Unix installer;
        // its official Windows instructions intentionally point at this main-branch script.
        cli_install_commands_windows: &["powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"$env:CONFIGURE='false'; irm https://raw.githubusercontent.com/aaif-goose/goose/main/download_cli.ps1 | iex\""],
        adapter_install_commands: &[],
        cli_install_instructions_url: "https://goose-docs.ai/docs/getting-started/installation/",
        adapter_install_instructions_url: "",
        cli_install_hint: "Buzz requires the Goose CLI; the desktop app alone is not enough.",
        adapter_install_hint: "",
        skill_dir: Some(".goose/skills"),
        supports_acp_model_switching: false,
        model_env_var: Some("GOOSE_MODEL"),
        provider_env_var: Some("GOOSE_PROVIDER"),
        provider_locked: false,
        default_env: &[("GOOSE_MODE", "auto")],
        config_file_path: Some("~/.config/goose/config.yaml"),
        config_file_format: Some("yaml"),
        supports_acp_native_config: true,
        thinking_env_var: Some("GOOSE_THINKING_EFFORT"),
        max_tokens_env_var: Some("GOOSE_MAX_TOKENS"),
        context_limit_env_var: Some("GOOSE_CONTEXT_LIMIT"),
        required_normalized_fields: &["model", "provider"],
        login_hint: None,
        auth_probe_args: None,
        provider_selection: true,
        capability_transport: HARNESS_MANAGED_TRANSPORT,
    },
    KnownAcpRuntime {
        id: "claude",
        label: "Claude Code",
        commands: &["claude-agent-acp", "claude-code-acp"],
        aliases: &["claude-code", "claudecode"],
        avatar_url: CLAUDE_CODE_AVATAR_URL,
        mcp_command: None,
        mcp_hooks: false,
        underlying_cli: Some("claude"),
        cli_install_commands: &["curl -fsSL https://claude.ai/install.sh | bash"],
        cli_install_commands_windows: &["powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"irm https://claude.ai/install.ps1 | iex\""],
        adapter_install_commands: &["npm install -g @agentclientprotocol/claude-agent-acp"],
        cli_install_instructions_url: "https://code.claude.com/docs/en/getting-started",
        adapter_install_instructions_url: "https://github.com/agentclientprotocol/claude-agent-acp",
        cli_install_hint: "Buzz requires the Claude Code CLI; the desktop app alone is not enough.",
        adapter_install_hint: "Install the Claude Code ACP adapter via npm.",
        skill_dir: Some(".claude/skills"),
        supports_acp_model_switching: false,
        model_env_var: None,
        provider_env_var: None,
        provider_locked: true,
        default_env: &[],
        config_file_path: Some("~/.claude/settings.json"),
        config_file_format: Some("json"),
        supports_acp_native_config: false,
        thinking_env_var: None,
        max_tokens_env_var: None,
        context_limit_env_var: None,
        required_normalized_fields: &[],
        login_hint: Some("Run the Claude CLI to complete authentication."),
        auth_probe_args: Some(&["claude", "auth", "status"]),
        provider_selection: false,
        capability_transport: HARNESS_MANAGED_TRANSPORT,
    },
    KnownAcpRuntime {
        id: "codex",
        label: "Codex",
        commands: &["codex-acp"],
        aliases: &[],
        avatar_url: CODEX_AVATAR_URL,
        mcp_command: Some("buzz-dev-mcp"),
        mcp_hooks: false,
        underlying_cli: Some("codex"),
        cli_install_commands: &["curl -fsSL https://chatgpt.com/codex/install.sh | sh"],
        cli_install_commands_windows: &["powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"irm https://chatgpt.com/codex/install.ps1 | iex\""],
        adapter_install_commands: &["npm install -g @agentclientprotocol/codex-acp"],
        cli_install_instructions_url: "https://developers.openai.com/codex/cli/",
        adapter_install_instructions_url: "https://github.com/agentclientprotocol/codex-acp",
        cli_install_hint: "Buzz requires the Codex CLI; the desktop app alone is not enough.",
        adapter_install_hint: "Install the Codex ACP adapter via npm.",
        skill_dir: Some(".codex/skills"),
        supports_acp_model_switching: false,
        model_env_var: None,
        provider_env_var: None,
        provider_locked: false,
        default_env: &[],
        config_file_path: Some("~/.codex/config.toml"),
        config_file_format: Some("toml"),
        supports_acp_native_config: false,
        thinking_env_var: None,
        max_tokens_env_var: None,
        context_limit_env_var: None,
        required_normalized_fields: &[],
        login_hint: Some("Run `codex login` to authenticate."),
        // Verified: `codex login status` exits 0 when logged in, non-zero otherwise.
        auth_probe_args: Some(&["codex", "login", "status"]),
        provider_selection: false,
        capability_transport: HARNESS_MANAGED_TRANSPORT,
    },
    KnownAcpRuntime {
        id: "buzz-agent",
        label: "Buzz Agent",
        commands: &["buzz-agent"],
        aliases: &[],
        avatar_url: BUZZ_AGENT_AVATAR_URL,
        mcp_command: Some("buzz-dev-mcp"),
        mcp_hooks: true,
        underlying_cli: None,
        cli_install_commands: &[],
        cli_install_commands_windows: &[],
        adapter_install_commands: &[],
        cli_install_instructions_url: "https://github.com/block/buzz",
        adapter_install_instructions_url: "https://github.com/block/buzz",
        cli_install_hint: "Ships with the Buzz desktop app.",
        adapter_install_hint: "",
        skill_dir: None,
        supports_acp_model_switching: true,
        model_env_var: Some("BUZZ_AGENT_MODEL"),
        provider_env_var: Some("BUZZ_AGENT_PROVIDER"),
        provider_locked: false,
        default_env: &[],
        config_file_path: None,
        config_file_format: None,
        supports_acp_native_config: false,
        thinking_env_var: Some("BUZZ_AGENT_THINKING_EFFORT"),
        max_tokens_env_var: Some("BUZZ_AGENT_MAX_OUTPUT_TOKENS"),
        context_limit_env_var: Some("BUZZ_AGENT_MAX_CONTEXT_TOKENS"),
        required_normalized_fields: &["model", "provider"],
        login_hint: None,
        auth_probe_args: None,
        provider_selection: true,
        capability_transport: HARNESS_MANAGED_TRANSPORT,
    },
    KnownAcpRuntime {
        id: "omp",
        label: "Oh My Pi",
        commands: &["omp"],
        aliases: &["oh-my-pi"],
        avatar_url: OMP_AVATAR_URL,
        mcp_command: None,
        mcp_hooks: false,
        // `omp` is the CLI itself; the ACP server is the built-in `omp acp`
        // subcommand, so there is no separate underlying CLI or npm adapter.
        underlying_cli: None,
        cli_install_commands: &["curl -fsSL https://omp.sh/install | sh"],
        cli_install_commands_windows: &["powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"irm https://omp.sh/install.ps1 | iex\""],
        adapter_install_commands: &[],
        cli_install_instructions_url: "https://github.com/can1357/oh-my-pi",
        adapter_install_instructions_url: "https://github.com/can1357/oh-my-pi",
        cli_install_hint: "Install the omp CLI via the official install script.",
        adapter_install_hint: "",
        skill_dir: Some(".omp/skills"),
        supports_acp_model_switching: false,
        // Model/provider/effort are ACP-native: omp exposes them as
        // session/new configOptions (categories model/mode/thought_level),
        // not as environment variables.
        model_env_var: None,
        provider_env_var: None,
        provider_locked: false,
        default_env: &[],
        config_file_path: Some("~/.omp/agent/config.yml"),
        config_file_format: Some("yaml"),
        supports_acp_native_config: false,
        thinking_env_var: None,
        max_tokens_env_var: None,
        context_limit_env_var: None,
        required_normalized_fields: &[],
        // Provider auth is per-model env keys or the omp auth broker; there is
        // no single login-status probe.
        login_hint: None,
        auth_probe_args: None,
        // omp selects its provider implicitly through the ACP model
        // configOption (model ids embed the provider); there is no provider
        // descriptor channel — no provider_env_var and no provider configOption
        // category — so a provider picker value would have no spawn path to
        // reach the harness (false parity). Stays false; this preserves the
        // pre-refactor `runtimeSupportsLlmProviderSelection("omp") == false`
        // behavior.
        provider_selection: false,
        // HARNESS-MANAGED (HC-001, `docs/hc001-omp-capability-transport.md`):
        // omp 17.1.6's `--tools`/`--no-tools`/`--no-skills` flags parse but do
        // not enforce the selected set in `omp acp` mode (enabling `read`
        // unlocks writes; skill/lesson tools survive `--no-tools`), so an
        // explicit tool policy would be false capability parity. Skills still
        // deliver via composed prompt sections like the other known runtimes.
        capability_transport: HARNESS_MANAGED_TRANSPORT,
    },
];

#[cfg(test)]
mod tests {
    use super::super::known_acp_runtime_exact;

    #[test]
    fn vendor_metadata_distinguishes_cli_and_adapter_guidance() {
        let goose = known_acp_runtime_exact("goose").unwrap();
        assert_eq!(
            goose.cli_install_instructions_url,
            "https://goose-docs.ai/docs/getting-started/installation/"
        );
        assert!(goose.adapter_install_instructions_url.is_empty());
        assert!(goose.cli_install_hint.contains("Goose CLI"));
        assert!(goose
            .cli_install_commands_windows
            .iter()
            .any(|command| command.contains("raw.githubusercontent.com/aaif-goose/goose/main")));
        assert!(goose
            .cli_install_commands_windows
            .iter()
            .any(|command| command.contains("$env:CONFIGURE='false'")));

        let claude = known_acp_runtime_exact("claude").unwrap();
        assert_eq!(
            claude.cli_install_instructions_url,
            "https://code.claude.com/docs/en/getting-started"
        );
        assert!(claude
            .adapter_install_instructions_url
            .contains("claude-agent-acp"));
        assert!(claude.cli_install_hint.contains("Claude Code CLI"));

        let codex = known_acp_runtime_exact("codex").unwrap();
        assert_eq!(
            codex.cli_install_instructions_url,
            "https://developers.openai.com/codex/cli/"
        );
        assert!(codex.adapter_install_instructions_url.contains("codex-acp"));
        assert!(codex.cli_install_hint.contains("Codex CLI"));
    }
}
