<script>
  // One cascade setting, every rung with its value and status (spec §5):
  // what we see (effective), what we modify (●), and why nothing happens
  // (outranked, or rejected with a reason).
  let { setting, editing = [], report = null, state = "none", failure = null, stale = false } = $props();

  const LABEL = {
    override: "override", device_final: "device final", screen: "screen",
    device_base: "device base", panel: "panel", fallback: "fallback",
  };
  const DEFAULT_ORDER = ["override", "device_final", "screen", "device_base", "panel", "fallback"];

  // The server lists rungs highest first; the widget reads lowest first.
  const order = $derived([...(report?.order ?? DEFAULT_ORDER)].reverse());
  function fmt(v) {
    if (v === undefined || v === null) return "—";
    if (Array.isArray(v)) return v.join(",");
    return String(v);
  }
  const notes = $derived(
    editing
      .map((rung) => {
        const r = report?.rungs?.[rung];
        if (r?.status === "outranked") return `your ${LABEL[rung]} value is not used in this preview`;
        if (r?.status === "rejected") return `your ${LABEL[rung]} value was rejected: ${r.reason}`;
        return null;
      })
      .filter(Boolean),
  );
</script>

<section class="flex flex-col gap-0.5 text-xs" data-testid={`ladder-${setting}`} aria-label={`${setting} ladder`}>
  {#if state === "none"}
    <p class="text-[var(--text-muted)]">No render yet.</p>
  {:else if state === "failed"}
    <p role="alert" class="text-[var(--danger-color)]">Last render failed: {failure}</p>
  {:else}
    {#if stale}<p class="text-[var(--text-muted)]">From the last render — Preview is closed.</p>{/if}
    <ol class="flex flex-col font-mono">
      {#each order as rung (rung)}
        {@const r = report?.rungs?.[rung]}
        <li data-rung={rung} data-status={r?.status ?? "absent"}
            class="grid grid-cols-[1rem_7rem_1fr_auto] gap-1 {r?.status === 'winner' ? 'font-semibold' : ''}">
          <span>{editing.includes(rung) ? "●" : ""}</span>
          <span>{LABEL[rung]}</span>
          <span class="truncate" title={fmt(r?.value)}>{fmt(r?.value)}</span>
          <span class="text-[var(--text-muted)]">
            {#if r?.status === "winner"}← wins{:else if r?.status === "rejected"}rejected{:else if r?.status === "outranked"}outranked{/if}
            {#if editing.includes(rung)} ← editing{/if}
          </span>
        </li>
      {/each}
      <li data-testid="effective">▶ effective {fmt(report?.effective)} from {LABEL[report?.from] ?? "—"}</li>
    </ol>
    {#each notes as n (n)}<p role="status" class="text-[var(--danger-color)]">{n}</p>{/each}
  {/if}
</section>
