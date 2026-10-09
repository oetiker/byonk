<script>
  import { untrack } from "svelte";
  import { Button } from "$lib/ui/button/index.js";
  import { Input } from "$lib/ui/input/index.js";

  /**
   * A one-field name form that replaces a native `prompt()`. Enter submits
   * the trimmed name, Escape cancels, a blank name is refused, and the focus
   * lands in the input when the form appears.
   */
  let { label, value = "", submitLabel = "OK", onsubmit, oncancel } = $props();

  // The prop is only the starting text; the user's typing owns it afterwards.
  let name = $state(untrack(() => value));
  let input = $state(null);
  const trimmed = $derived(name.trim());

  $effect(() => {
    input?.focus();
  });

  function submit() {
    if (trimmed !== "") onsubmit(trimmed);
  }

  function onkeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      oncancel();
    } else if (e.key === "Enter" && e.target === input) {
      // preventDefault on keydown suppresses the browser's implicit submit.
      e.preventDefault();
      submit();
    }
  }
</script>

<form
  aria-label={label}
  class="flex min-w-0 items-center gap-1 border-b border-[var(--border-color)] p-1 text-xs"
  {onkeydown}
  onsubmit={(e) => {
    e.preventDefault();
    submit();
  }}
>
  <Input bind:ref={input} bind:value={name} aria-label={label} class="h-6 min-w-0 flex-1 text-xs" />
  <Button type="submit" size="xs" variant="outline" disabled={trimmed === ""}>{submitLabel}</Button>
  <Button type="button" size="xs" variant="outline" onclick={() => oncancel()}>Cancel</Button>
</form>
