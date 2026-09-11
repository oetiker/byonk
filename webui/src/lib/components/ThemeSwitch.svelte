<script>
  // Three-way colour scheme control (spec §8.9). Icon-only trigger, not a
  // labelled text control: at 32px the rail has no room for "System" /
  // "Light" / "Dark" text, so the icon carries the CURRENT choice and the
  // full labels live in the popover the Select opens, plus an `aria-label`
  // that is always up to date for anyone not reading the icon.
  import { Monitor, Sun, Moon } from "@lucide/svelte";
  import * as Select from "../ui/select/index.js";
  import { theme } from "../state/theme.svelte.js";

  const MODES = ["system", "light", "dark"];
  const ICONS = { system: Monitor, light: Sun, dark: Moon };
  const LABELS = { system: "System", light: "Light", dark: "Dark" };

  let Icon = $derived(ICONS[theme.mode]);
</script>

<Select.Root
  type="single"
  value={theme.mode}
  onValueChange={(mode) => theme.set(mode)}
>
  <Select.Trigger
    aria-label={`Theme: ${LABELS[theme.mode]}`}
    class="h-8 w-9 gap-0.5 justify-center rounded border-0 py-0 pr-0 pl-0
           bg-transparent text-[var(--text-muted)] [&_svg]:size-3.5
           hover:bg-[var(--bg-color)] hover:text-[var(--text-color)]
           data-[state=open]:bg-[var(--bg-color)]
           data-[state=open]:text-[var(--accent-color)]"
  >
    <Icon aria-hidden="true" />
  </Select.Trigger>
  <Select.Content>
    {#each MODES as mode (mode)}
      <Select.Item value={mode} label={LABELS[mode]} />
    {/each}
  </Select.Content>
</Select.Root>
