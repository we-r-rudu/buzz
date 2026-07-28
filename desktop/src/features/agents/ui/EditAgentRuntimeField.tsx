/**
 * Runtime ("Provider") dropdown + custom-command field for the instance edit
 * dialog. Extracted as pure presentation (file-size guard); every runtime
 * side effect (command prefill, inherit-transition, selection reset) stays in
 * the dialog's runtime-change handler, which is passed in as `onRuntimeChange`.
 */
import * as React from "react";

import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";
import { PersonaDropdownField } from "./PersonaDropdownField";
import {
  formatRuntimeOptionLabel,
  NO_RUNTIME_DROPDOWN_VALUE,
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
  sortPersonaRuntimes,
  type PersonaDropdownOption,
} from "./agentConfigOptions";

export function EditAgentRuntimeField({
  agentCommand,
  disabled,
  inheritHarness,
  onAgentCommandChange,
  onRuntimeChange,
  runtimes,
  selectedRuntime,
  selectedRuntimeId,
}: {
  agentCommand: string;
  disabled?: boolean;
  inheritHarness: boolean;
  onAgentCommandChange: (next: string) => void;
  onRuntimeChange: (nextValue: string) => void;
  runtimes: AcpRuntimeCatalogEntry[];
  selectedRuntime: AcpRuntimeCatalogEntry | undefined;
  selectedRuntimeId: string;
}) {
  const sortedRuntimes = React.useMemo(
    () => sortPersonaRuntimes(runtimes),
    [runtimes],
  );
  const runtimeDropdownValue = selectedRuntimeId || NO_RUNTIME_DROPDOWN_VALUE;
  const runtimeDropdownOptions: PersonaDropdownOption[] = React.useMemo(() => {
    const options: PersonaDropdownOption[] = [
      ...sortedRuntimes.map((candidate) => ({
        label: formatRuntimeOptionLabel(candidate),
        value: candidate.id,
      })),
      { label: "Custom command", value: "custom" },
    ];
    if (
      selectedRuntimeId &&
      selectedRuntimeId !== "custom" &&
      !options.some((o) => o.value === selectedRuntimeId)
    ) {
      options.push({
        label: `${selectedRuntimeId} (current)`,
        value: selectedRuntimeId,
      });
    }
    return options;
  }, [sortedRuntimes, selectedRuntimeId]);

  return (
    <>
      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="edit-agent-runtime"
        >
          Provider
        </label>
        <PersonaDropdownField
          disabled={disabled}
          id="edit-agent-runtime"
          onValueChange={onRuntimeChange}
          options={runtimeDropdownOptions}
          placeholder="Choose a provider"
          value={runtimeDropdownValue}
        />
        {selectedRuntime ? (
          <p className="text-xs text-muted-foreground">
            Detected at{" "}
            <span className="font-medium">
              {selectedRuntime.binaryPath ??
                selectedRuntime.command ??
                selectedRuntime.id}
            </span>
          </p>
        ) : null}
      </div>
      {selectedRuntimeId === "custom" && !inheritHarness ? (
        <div className="space-y-1.5">
          <label
            className="text-sm font-medium text-foreground"
            htmlFor="edit-agent-command"
          >
            Agent command
          </label>
          <div
            className={cn(
              "flex min-h-11 items-center px-3",
              PERSONA_FIELD_SHELL_CLASS,
            )}
          >
            <Input
              autoCorrect="off"
              className={cn(
                "h-8 px-0 py-0 leading-6",
                PERSONA_FIELD_CONTROL_CLASS,
              )}
              disabled={disabled}
              id="edit-agent-command"
              onChange={(event) => onAgentCommandChange(event.target.value)}
              placeholder="Full path or shell command"
              value={agentCommand}
            />
          </div>
        </div>
      ) : null}
    </>
  );
}
