# OMP agent integration and single-agent VPS deployment

This document separates software that exists now from the strict OMP RPC integration that still has to be built.

- **Current** means the cited code is present in this fork or in OMP today.
- **Current, optional** means it can be used now, but it is not the requested strict non-ACP design.
- **Proposed** means a design contract, not an implemented Buzz feature.

## Current: set up this fork locally

The fork follows the upstream Hermit and `just` workflow. Docker is required. Hermit supplies the pinned Rust, Node.js, pnpm, and `just` toolchain; without Hermit, use the minimum versions in [CONTRIBUTING.md](../CONTRIBUTING.md#prerequisites).

```bash
git clone https://github.com/we-r-rudu/buzz.git
cd buzz
. ./bin/activate-hermit
just setup
just build
```

`just setup` is safe to rerun. It downloads the pinned tools, creates `.env` from `.env.example` only when `.env` is absent, starts the local services, applies migrations, and installs desktop dependencies. These steps are defined by [the `bootstrap` and `setup` recipes](../Justfile#L24-L48) and described in the [contributor setup](../CONTRIBUTING.md#first-time-setup).

For normal development:

```bash
. ./bin/activate-hermit
just dev
```

This builds the agent-side binaries, starts the relay at `ws://localhost:3000`, waits for readiness, and starts the Tauri desktop app. The recipe owns the relay process and stops it when the desktop exits ([Justfile](../Justfile#L403-L462)). To keep the relay in a separate terminal, run `just relay`; use `just down` to stop the Docker services without deleting their data ([Justfile](../Justfile#L71-L81), [README.md](../README.md#quick-start)).

At the time of writing, [this fork has no packaged releases](https://github.com/we-r-rudu/buzz/releases). Its OSS download link still points to the [upstream `block/buzz` releases](../README.md#i-just-want-to-install-it), so an upstream DMG, AppImage, package, or installer is not evidence that it contains this fork's changes. Build from source with the commands above when the exact `we-r-rudu/buzz` revision matters.

Do not put production credentials in the repository `.env`. It is a local development template, and its documented agent quick start uses `BUZZ_PRIVATE_KEY` and `BUZZ_RELAY_URL` from the environment ([.env.example](../.env.example#L112-L139)).

## Current: Buzz has an ACP subprocess seam

The agent path implemented today is:

```text
Buzz relay  <---- WebSocket ---->  buzz-acp  <---- ACP over stdio ---->  agent runtime
                                         |
                                         +---- Buzz CLI / optional MCP sidecar
```

`buzz-acp` discovers channels available to the agent identity, subscribes to relevant relay events, queues work per channel, and calls ACP `initialize`, `session/new`, `session/prompt`, and `session/cancel`. An agent must return a session ID, stream `session/update` notifications, and finish a prompt with a `stopReason` ([buzz-acp README](../crates/buzz-acp/README.md#using-any-acp-agent), [ACP client](../crates/buzz-acp/src/acp.rs#L535-L575), [prompt and cancel](../crates/buzz-acp/src/acp.rs#L672-L748)). The worker pool owns `AcpClient` instances and keeps channel-to-session state ([pool.rs](../crates/buzz-acp/src/pool.rs#L1-L20), [pool state](../crates/buzz-acp/src/pool.rs#L83-L163)). There is no OMP RPC transport in that path.

Claude Code does not speak directly to Buzz in the current desktop catalog. The selected commands are `claude-agent-acp` and the legacy `claude-code-acp`, and the catalog installs `@agentclientprotocol/claude-agent-acp` ([desktop discovery](../desktop/src-tauri/src/managed_agents/discovery.rs#L97-L127)). The desktop then starts `buzz-acp`, sets `BUZZ_ACP_AGENT_COMMAND`, `BUZZ_ACP_AGENT_ARGS`, relay credentials, MCP command, and worker count, and places the process tree in a locally owned process group ([desktop runtime](../desktop/src-tauri/src/managed_agents/runtime.rs#L1491-L1605), [worker and process-group setup](../desktop/src-tauri/src/managed_agents/runtime.rs#L1715-L1737)). In other words, Claude Code's current Buzz integration is an ACP adapter path.

## Current, optional: OMP can use ACP without a custom adapter

OMP itself already has an `omp acp` command that runs an ACP server over stdio. It is not necessary to write an `omp-acp` wrapper: OMP's command selects ACP mode directly ([OMP `acp` command](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/commands/acp.ts)). OMP accepts ACP `session/new` MCP servers and configures them in the new session ([OMP ACP agent](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/acp/acp-agent.ts)). That matches the current `buzz-acp` subprocess contract.

A source-compatible single-worker configuration is:

```bash
export BUZZ_RELAY_URL="wss://buzz.example.com"
export BUZZ_PRIVATE_KEY="<agent-nsec>"
export BUZZ_ACP_AGENT_COMMAND="omp"
export BUZZ_ACP_AGENT_ARGS="acp,--approval-mode,yolo"
export BUZZ_ACP_MCP_COMMAND="buzz-dev-mcp"
export BUZZ_ACP_AGENTS="1"

buzz-acp
```

`BUZZ_ACP_AGENT_ARGS` is comma-delimited, and the harness supports one through 32 subprocesses ([buzz-acp configuration](../crates/buzz-acp/README.md#core), [agent count](../crates/buzz-acp/README.md#parallel-agents--heartbeat)). OMP's `yolo` approval mode skips tool approval prompts; use it only inside an appropriately restricted service account or container ([OMP approval flag](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/commands/launch.ts)). For an interactive approval policy, omit that flag and provide an approval-capable host.

This route is **ACP end to end**. It is useful for proving credentials, relay access, channel membership, OMP authentication, and workload sizing, but it does not satisfy the strict non-ACP requirement below.

## Proposed: strict OMP RPC path, with no ACP layer

**Status: not implemented.** Setting `BUZZ_ACP_AGENT_COMMAND=omp` with `--mode rpc-ui` will not work because `buzz-acp` emits ACP JSON-RPC, while OMP RPC expects its own newline-delimited command frames. The strict path needs a native Buzz-side OMP RPC transport. It must reuse the relay subscription, author gate, channel queue, context construction, presence, typing, and recovery behavior that currently surround `AcpClient`; it must not translate RPC to ACP or start `buzz-acp` as an intermediary.

The child command is:

```bash
omp --mode rpc-ui --cwd /srv/buzz-agent/work
```

`rpc-ui` is a current OMP mode. It gives the session a host UI context while keeping RPC on stdin/stdout and disabling PTY behavior ([OMP mode type](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/cli/args.ts), [OMP startup dispatch](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/main.ts)). OMP emits one JSON line `{ "type": "ready" }` before accepting commands, then emits correlated responses and session events on stdout ([RPC server](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/rpc/rpc-mode.ts)). The wire types are first-party OMP interfaces in [`rpc-types.ts`](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/rpc/rpc-types.ts).

### Required command and event mapping

| Buzz responsibility | OMP RPC mapping |
|---|---|
| Child readiness | Wait for `{type:"ready"}`. Treat EOF, malformed frames, or child exit as worker failure and restart under the lifecycle supervisor. |
| Correlation | Put a unique `id` on every command. Match `{type:"response", id, command, success}`; a successful `prompt` response only acknowledges dispatch. It does not mean the turn is finished. |
| One session per Buzz channel | Create a channel session with `new_session`, then call `get_state` and retain its `sessionFile`. Use `switch_session` before returning to that channel. With one worker, queue other channels until the active turn finishes. |
| New Buzz event | Send `prompt {message}` when idle. If another eligible event arrives for the active channel during a turn, use `steer {message}`. Do not switch channels during a streaming turn. |
| Owner cancel or rotation | Map cancel to `abort`. Map rotation to a fresh `new_session` and replace only that channel's saved session. This preserves the current `!cancel` and `!rotate` distinction ([current control semantics](../crates/buzz-acp/README.md#inbound-author-gate)). |
| Model selection | Use `set_model {provider, modelId}` and require a successful correlated response before reporting the selection as active. |
| Buzz operations | Register a narrow Buzz tool surface with `set_host_tools`. Execute each `host_tool_call` in the Buzz-side host and answer with `host_tool_update` and exactly one terminal `host_tool_result`; honor `host_tool_cancel`. This is the non-ACP replacement for handing MCP servers to an ACP session. |
| Turn progress and liveness | Use `agent_start`, `turn_start`, `message_start/update/end`, and `tool_execution_start/update/end` as progress. Reset the idle timer on valid progress. Treat `agent_end` as normal turn completion. The event shapes come from OMP's [`AgentSessionEvent`](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/session/agent-session.ts) and core [`AgentEvent`](https://github.com/can1357/oh-my-pi/blob/main/packages/agent-core/src/types.ts). |
| Local-only prompt | If OMP emits `prompt_result` with `agentInvoked:false`, finish the dispatch without waiting for `agent_end`; OMP documents this frame separately from the immediate prompt response ([RPC prompt result](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/rpc/rpc-types.ts#L124-L133)). |
| Approval and other UI requests | Handle every `extension_ui_request` and return a matching `extension_ui_response`. If no authorized operator can answer, cancel or time out the request rather than silently approving it. An explicitly configured `--approval-mode yolo` is an operational alternative only inside a restricted runtime. |

Assistant stream events are observability data, not Buzz messages. Outbound channel changes and messages should occur only through the registered Buzz host tools so one model turn cannot be posted twice. Keep the Nostr signing key in the Buzz-side host; OMP needs the tool contract, not the raw identity secret. OMP's host-tool request/result fields are defined in the [RPC host-tool frames](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/rpc/rpc-types.ts#L387-L430).

Before calling this path complete, verify ready-frame handling, per-channel session restoration, cancellation, host-tool success and failure, approval timeout, child restart, relay reconnect with replay, and a real mention-to-reply flow. None of those strict-path checks are evidence of current support until the native transport exists.

## Single-agent VPS topology

The deployment target is one Buzz identity, one transport process, and one OMP child. Do not scale the service, run a second replica, or set the current ACP worker count above one.

```text
Buzz desktop/mobile ---- WSS ----+
                                 +---- Buzz relay
single-agent VPS ------- WSS ----+
  one Buzz transport
    one channel queue
    one OMP child over stdio
```

The VPS does not need an inbound agent port. It needs outbound DNS and HTTPS access to the model provider and an outbound **WSS** connection to the relay. Use `wss://` for every non-loopback relay connection; the local `ws://localhost:3000` default is only suitable for local development. `BUZZ_RELAY_URL` is the harness connection target, distinct from the relay server's own `RELAY_URL` setting ([.env.example](../.env.example#L42-L50), [agent relay setting](../.env.example#L121-L139)).

### Prerequisites and secrets

1. Build `buzz-acp`, `buzz`, and `buzz-dev-mcp` in release mode for the current optional ACP deployment. Install OMP using its [official installation instructions](https://github.com/can1357/oh-my-pi#install), then authenticate the chosen provider or supply its documented API credential.
2. Mint a dedicated agent identity. The current command is `cargo run -p buzz-admin -- mint-token --name "omp-agent" --scopes "messages:read,messages:write,channels:read"`; it prints the private key and API token once ([buzz-acp key generation](../crates/buzz-acp/README.md#generating-keys)). Never reuse a human key.
3. Add the agent identity to every intended channel before starting the service. The harness discovers channels as the authenticated agent and defaults to member channels; private channels require explicit membership ([buzz-acp channels](../crates/buzz-acp/README.md#channels)). Keep `BUZZ_ACP_CHANNELS` or the future RPC driver's equivalent narrowed when the deployment should see only a subset ([.env.example](../.env.example#L182-L196)).
4. Store `BUZZ_PRIVATE_KEY`, an optional `BUZZ_API_TOKEN` when relay policy requires it, and provider credentials in a root-owned secret file or container secret. Do not bake them into an image, unit file, shell history, or repository. The strict proposed driver should retain the Buzz key and perform signed host-tool operations itself.
5. Restrict who can prompt the agent. The current harness defaults to `owner-only` and supports a pubkey allowlist; do not use `anyone` unless an open agent is intentional ([author gate](../crates/buzz-acp/README.md#inbound-author-gate)).

### Current, optional: systemd lifecycle

The following unit runs the deployable ACP route above with exactly one worker. Paths are installation choices and must match the VPS. Put the secret values in `/etc/buzz/omp-agent.env`, owned by root with mode `0600`.

```ini
[Unit]
Description=Buzz OMP agent (single worker, ACP path)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=buzz-agent
Group=buzz-agent
WorkingDirectory=/srv/buzz-agent/work
Environment=PATH=/opt/buzz/bin:/usr/local/bin:/usr/bin
Environment=HOME=/srv/buzz-agent
Environment=BUZZ_ACP_AGENT_COMMAND=/usr/local/bin/omp
Environment=BUZZ_ACP_AGENT_ARGS=acp,--approval-mode,yolo
Environment=BUZZ_ACP_MCP_COMMAND=/opt/buzz/bin/buzz-dev-mcp
Environment=BUZZ_ACP_AGENTS=1
EnvironmentFile=/etc/buzz/omp-agent.env
ExecStart=/opt/buzz/bin/buzz-acp
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/srv/buzz-agent

[Install]
WantedBy=multi-user.target
```

Use `systemctl enable --now buzz-omp-agent` after installing the unit, inspect logs with `journalctl -u buzz-omp-agent`, and use `systemctl restart` after changing credentials or configuration. `Restart=on-failure` is the supervisor boundary; its semantics are documented by [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html).

A container deployment should run the same command as PID 1 through a minimal init, mount one writable work/session volume, inject secrets at runtime, use a failure restart policy, and declare exactly one replica. Do not place the relay or provider credentials in the image or Compose file. Docker documents restart policy behavior in [Start containers automatically](https://docs.docker.com/engine/containers/start-containers-automatically/). There is no repository-provided strict-RPC agent image or entrypoint today.

For the proposed strict path, replace this ACP service only after a real Buzz OMP RPC transport exists and passes the end-to-end checks above. Do not point this unit's `buzz-acp` executable at `omp --mode rpc-ui`; the protocols are different.

### Current limitation: desktop controls only local children

Desktop start and stop are local process-management operations. The app records a local PID, keeps locally spawned children in an in-memory runtime map, stamps them with a desktop-instance `BUZZ_MANAGED_AGENT` marker, and sends process-group termination signals on stop ([local spawn ownership](../desktop/src-tauri/src/managed_agents/runtime.rs#L1876-L1905), [start and stop](../desktop/src-tauri/src/managed_agents/runtime.rs#L1966-L2068)). A standalone process supervised by systemd or a container on a VPS has none of that local ownership state.

Consequently, the current desktop Start/Stop buttons do **not** start, stop, restart, or report authoritative process health for a standalone VPS agent. Operate that process through systemd or the container runtime. Remote lifecycle control would require a separate authenticated control plane; it is not part of either the current ACP path or this proposed strict RPC transport.
