import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
} from "./agentConfigOptions";

/**
 * Name + instructions fields for the agent definition dialog, split from
 * `AgentDefinitionDialog.tsx` (file-size guard). Pure presentation — the
 * dialog owns the state.
 */
export function PersonaBasicsFields({
  displayName,
  systemPrompt,
  disabled,
  onDisplayNameChange,
  onSystemPromptChange,
}: {
  displayName: string;
  systemPrompt: string;
  disabled?: boolean;
  onDisplayNameChange: (value: string) => void;
  onSystemPromptChange: (value: string) => void;
}) {
  return (
    <>
      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="persona-display-name"
        >
          Agent name
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
            id="persona-display-name"
            onChange={(event) => onDisplayNameChange(event.target.value)}
            placeholder="Fizz"
            value={displayName}
          />
        </div>
      </div>

      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="persona-system-prompt"
        >
          Agent instructions
        </label>
        <div className={PERSONA_FIELD_SHELL_CLASS}>
          <Textarea
            className={cn(
              "min-h-40 resize-y px-3 py-3 leading-5",
              PERSONA_FIELD_CONTROL_CLASS,
            )}
            disabled={disabled}
            id="persona-system-prompt"
            onChange={(event) => onSystemPromptChange(event.target.value)}
            placeholder="Describe what this agent should do."
            value={systemPrompt}
          />
        </div>
      </div>
    </>
  );
}
