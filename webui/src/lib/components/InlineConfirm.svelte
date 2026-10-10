<script>
  import { Button } from "$lib/ui/button/index.js";

  /**
   * A destructive-action confirmation that replaces a native `confirm()`.
   * The message names what is affected; the focus lands on the confirm
   * button, Escape or Cancel dismisses.
   */
  let { label, message, confirmLabel = "Delete", onconfirm, oncancel } = $props();

  let confirmButton = $state(null);

  $effect(() => {
    confirmButton?.focus();
  });

  function onkeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      oncancel();
    } else if (e.key === "Enter" && e.target === confirmButton) {
      e.preventDefault();
      onconfirm();
    }
  }
</script>

<form
  aria-label={label}
  class="flex min-w-0 flex-wrap items-center gap-1 border-b border-border p-1 text-xs"
  {onkeydown}
  onsubmit={(e) => {
    e.preventDefault();
    onconfirm();
  }}
>
  <p class="min-w-0 flex-1 break-words">{message}</p>
  <Button bind:ref={confirmButton} type="submit" size="xs" variant="destructive">{confirmLabel}</Button>
  <Button type="button" size="xs" variant="outline" onclick={() => oncancel()}>Cancel</Button>
</form>
