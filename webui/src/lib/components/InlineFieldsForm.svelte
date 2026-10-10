<script>
  import { Button } from "$lib/ui/button/index.js";

  /**
   * The several-field sibling of `InlineNameForm`: the same frame and the
   * same keys. Enter in a field submits, Escape cancels, the first field has
   * the focus when the form appears. The fields are the caller's (`children`),
   * and so is their state -- a failed submit leaves the form open with every
   * value as typed. `canSubmit` gates OK and Enter alike.
   */
  let { label, canSubmit = true, submitLabel = "OK", onsubmit, oncancel, children } = $props();

  let form = $state(null);

  $effect(() => {
    form?.querySelector("input, select, textarea")?.focus();
  });

  function submit() {
    if (canSubmit) onsubmit();
  }

  function onkeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      oncancel();
    } else if (e.key === "Enter" && ["INPUT", "SELECT"].includes(e.target.tagName)) {
      // preventDefault on keydown suppresses the browser's implicit submit.
      // Enter on a button inside the fields keeps its own meaning (click).
      e.preventDefault();
      submit();
    }
  }
</script>

<form
  bind:this={form}
  aria-label={label}
  class="flex min-w-0 flex-col gap-1 border-b border-border p-1 text-xs"
  {onkeydown}
  onsubmit={(e) => {
    e.preventDefault();
    submit();
  }}
>
  {@render children()}
  <div class="flex items-center justify-end gap-1">
    <Button type="submit" size="xs" variant="outline" disabled={!canSubmit}>{submitLabel}</Button>
    <Button type="button" size="xs" variant="outline" onclick={() => oncancel()}>Cancel</Button>
  </div>
</form>
