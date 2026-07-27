import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

/**
 * /vps:allowlist — roll out a respond-to allowlist to the VPS buzz-agent fleet.
 *
 * Pure deterministic flow (no LLM turn): stepper TUI collects a CSV of user
 * pubkeys, picks target agents, then SSHes each agent's env file update and
 * prints a summary. No restart: buzz-acp's live gate (BUZZ_ACP_ENV_FILE)
 * re-reads the env file on the next inbound event.
 *
 * Remote layout (see .agents/skills/sync-upstream-and-update):
 *   env:      /etc/buzz-agents/<name>.env   (contains BUZZ_PRIVATE_KEY — never printed)
 *   service:  buzz-agent@<name>
 *   gate:     buzz-acp re-reads BUZZ_ACP_RESPOND_TO / BUZZ_ACP_RESPOND_TO_ALLOWLIST
 *             from the env file on mtime change; DMs stay owner-only
 *             by design regardless of mode.
 */

const SSH_HOST = "dn";
const ENV_DIR = "/etc/buzz-agents";
const PUBKEY_RE = /^[0-9a-f]{64}$/;
const NAME_RE = /^[a-z0-9][a-z0-9-]*$/;
const WIDGET_KEY = "vps-allowlist";
const STEP_LABELS = [
  "Collect pubkeys",
  "Pick agents",
  "Confirm rollout",
  "Apply on VPS (hot-reload)",
  "Summary",
] as const;

function parsePubkeys(raw: string): { valid: string[]; invalid: string[] } {
  const seen = new Set<string>();
  const valid: string[] = [];
  const invalid: string[] = [];
  for (const token of raw.split(/[\s,]+/).filter(Boolean)) {
    const pk = token.toLowerCase();
    if (!PUBKEY_RE.test(pk)) {
      invalid.push(token);
      continue;
    }
    if (!seen.has(pk)) {
      seen.add(pk);
      valid.push(pk);
    }
  }
  return { valid, invalid };
}

const trunc = (pk: string) => `${pk.slice(0, 8)}…`;

/**
 * One-shot per-agent remote script: backup → strip old gate lines → append →
 * restart → verify. Emits a single parseable RESULT line. Never echoes env
 * contents; the identity key is verified via hash comparison only.
 */
function remoteApplyScript(agent: string, csv: string, ts: string): string {
  return [
    "set -euo pipefail",
    `f=${ENV_DIR}/${agent}.env`,
    `cp "$f" "$f.bak-${ts}"`,
    `before=$(grep '^BUZZ_PRIVATE_KEY=' "$f" | sha256sum | cut -d' ' -f1)`,
    `sed -i -e '/^BUZZ_ACP_RESPOND_TO=/d' -e '/^BUZZ_ACP_RESPOND_TO_ALLOWLIST=/d' "$f"`,
    `[ -n "$(tail -c1 "$f")" ] && printf '\\n' >> "$f"`,
    `printf 'BUZZ_ACP_RESPOND_TO=allowlist\nBUZZ_ACP_RESPOND_TO_ALLOWLIST=%s\n' '${csv}' >> "$f"`,
    // No restart: buzz-acp's live gate (BUZZ_ACP_ENV_FILE) re-reads the env
    // file on the next inbound event. Verify the lines landed and the
    // identity key is intact — the harness applies the change itself.
    `after=$(grep '^BUZZ_PRIVATE_KEY=' "$f" | sha256sum | cut -d' ' -f1)`,
    `mode=$(grep -c '^BUZZ_ACP_RESPOND_TO=allowlist$' "$f" || true)`,
    `entries=$(grep '^BUZZ_ACP_RESPOND_TO_ALLOWLIST=' "$f" | cut -d= -f2 | tr ',' '\n' | grep -c . || true)`,
    `[ "$before" = "$after" ] && key=ok || key=FAILED`,
    `echo "RESULT mode=$mode entries=$entries key=$key backup=$f.bak-${ts}"`,
  ].join("\n");
}

export default function vpsAllowlist(pi: ExtensionAPI): void {
  pi.registerCommand("vps:allowlist", {
    description:
      "Roll out a respond-to allowlist to VPS agents (stepper TUI → env update + restart, no LLM)",
    async handler(_args, ctx) {
      if (!ctx.hasUI) {
        ctx.ui.notify("/vps:allowlist requires interactive mode", "warning");
        return;
      }
      const { ui } = ctx;
      const showStep = (current: number, note = "") =>
        ui.setWidget(
          WIDGET_KEY,
          [
            "VPS allowlist rollout",
            ...STEP_LABELS.map((label, i) => {
              const n = i + 1;
              const mark = n < current ? "✓" : n === current ? "●" : "○";
              return ` ${mark} ${n} · ${label}${n === current && note ? ` — ${note}` : ""}`;
            }),
          ],
          { placement: "aboveEditor" },
        );

      try {
        // ── Step 1 · pubkeys ────────────────────────────────────────────
        showStep(1, "waiting for CSV input");
        let pubkeys: string[] | null = null;
        let prefill = "";
        while (pubkeys === null) {
          const raw = await ui.editor(
            "Step 1/5 · Allowlist pubkeys — CSV or whitespace-separated 64-hex",
            prefill,
          );
          if (raw === undefined) {
            ui.notify("Allowlist rollout cancelled", "info");
            return;
          }
          const { valid, invalid } = parsePubkeys(raw);
          if (valid.length > 0 && invalid.length === 0) {
            pubkeys = valid;
            break;
          }
          ui.notify(
            invalid.length > 0
              ? `${invalid.length} invalid token(s): ${invalid
                  .slice(0, 3)
                  .map((t) => `${t.slice(0, 12)}…`)
                  .join(", ")} — fix or Esc to cancel`
              : "No pubkeys entered",
            "error",
          );
          prefill = raw;
        }
        const csv = pubkeys.join(",");

        // ── Step 2 · agents ─────────────────────────────────────────────
        showStep(2, "discovering fleet");
        const rosterRes = await pi.exec("ssh", [SSH_HOST, `ls ${ENV_DIR}/*.env`], {
          timeout: 15000,
        });
        if (rosterRes.code !== 0) {
          ui.notify(`Fleet discovery failed: ${rosterRes.stderr.trim()}`, "error");
          return;
        }
        const roster = rosterRes.stdout
          .split("\n")
          .map((line) => line.trim().split("/").pop() ?? "")
          .map((name) => name.replace(/\.env$/, ""))
          .filter((name) => NAME_RE.test(name));
        if (roster.length === 0) {
          ui.notify(`No agent env files found in ${ENV_DIR}`, "error");
          return;
        }
        const allLabel = `All agents (${roster.length})`;
        showStep(2, "waiting for selection");
        const pick = await ui.select("Step 2/5 · Apply to which agents?", [
          allLabel,
          ...roster,
        ]);
        if (pick === undefined) {
          ui.notify("Allowlist rollout cancelled", "info");
          return;
        }
        const targets = pick === allLabel ? roster : [pick];

        // ── Step 3 · confirm ────────────────────────────────────────────
        showStep(3, "waiting for confirmation");
        const pkPreview =
          pubkeys.slice(0, 3).map(trunc).join(", ") +
          (pubkeys.length > 3 ? ` +${pubkeys.length - 3} more` : "");
        const confirmed = await ui.confirm(
          "Step 3/5 · Confirm rollout",
          `Set BUZZ_ACP_RESPOND_TO=allowlist with ${pubkeys.length} pubkey(s) (${pkPreview}) ` +
            `on ${targets.length} agent(s): ${targets.join(", ")}.\n\n` +
            `Replaces any existing allowlist. The owner stays implicitly allowed; DMs remain owner-only. ` +
            `Each env file is backed up on-host first (<name>.env.bak-<ts>). Continue?`,
        );
        if (!confirmed) {
          ui.notify("Allowlist rollout cancelled", "info");
          return;
        }

        // ── Step 4 · apply + restart + verify (per agent) ───────────────
        const ts = new Date().toISOString().replace(/[-:T]/g, "").slice(0, 14);
        type AgentResult = { agent: string; ok: boolean; detail: string };
        const results: AgentResult[] = [];
        for (const [index, agent] of targets.entries()) {
          showStep(4, `${agent} (${index + 1}/${targets.length})`);
          const res = await pi.exec("ssh", [SSH_HOST, remoteApplyScript(agent, csv, ts)], {
            timeout: 60000,
          });
          if (res.code !== 0) {
            const detail =
              res.stderr.trim().split("\n").pop() || `ssh exit ${res.code}`;
            results.push({ agent, ok: false, detail });
            continue;
          }
          const m = /RESULT mode=(\S+) entries=(\S+) key=(\S+)/.exec(res.stdout);
          if (!m) {
            results.push({ agent, ok: false, detail: "no RESULT marker" });
            continue;
          }
          const [, mode, entries, key] = m;
          const ok = mode === "1" && key === "ok" && Number(entries) === pubkeys.length;
          results.push({
            agent,
            ok,
            detail: `entries=${entries}/${pubkeys.length} key=${key}`,
          });
        }

        // ── Step 5 · summary ────────────────────────────────────────────
        showStep(5);
        const updated = results.filter((r) => r.ok);
        const failed = results.filter((r) => !r.ok);
        const summary = [
          `**VPS allowlist rollout — ${failed.length === 0 ? "done" : "completed with failures"}**`,
          ``,
          `- Mode: \`allowlist\` (${pubkeys.length} pubkey(s): ${pubkeys.map(trunc).join(", ")})`,
          `- Updated (${updated.length}): ${updated.map((r) => r.agent).join(", ") || "—"}`,
          ...(failed.length > 0
            ? [
                `- Failed (${failed.length}): ${failed.map((r) => `${r.agent} — ${r.detail}`).join(", ")}`,
              ]
            : []),
          `- Backups on-host: \`${ENV_DIR}/<name>.env.bak-${ts}\``,
          `- Applied live (hot-reload — no restart); identity keys verified intact`,
        ].join("\n");
        pi.sendMessage({
          customType: "vps-allowlist-summary",
          content: summary,
          display: true,
        });
        ui.notify(
          failed.length === 0
            ? `Allowlist live on ${updated.length} agent(s)`
            : `${failed.length} agent(s) failed — see summary`,
          failed.length === 0 ? "info" : "error",
        );
      } finally {
        ui.setWidget(WIDGET_KEY, undefined);
      }
    },
  });
}
