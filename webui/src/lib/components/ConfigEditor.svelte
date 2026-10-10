<script>
  import { onDestroy } from "svelte";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft, SCHEMAS } from "../state/configDraft.svelte.js";
  import { resolution } from "../state/resolution.svelte.js";
  import { layout } from "../state/layout.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import Ladder from "./Ladder.svelte";
  import SwatchEditor from "./SwatchEditor.svelte";
  import { Button } from "$lib/ui/button/index.js";
  import { Input } from "$lib/ui/input/index.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Circle } from "@lucide/svelte";
  import PaneHeader from "./PaneHeader.svelte";

  const selectClass =
    "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm " +
    "text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 " +
    "focus-visible:ring-ring/50 dark:bg-input/30";
  const RUNG_LABEL = { panel: "panel", device_base: "device base", device_final: "device final" };
  const STEP = { max_error: 0.05, noise_scale: 0.5, chroma_clamp: 0.05, strength: 0.05,
                 "gamut.knee": 0.05, "gamut.amount": 0.05, "gamut.max_compression": 0.1, refresh: 60 };

  const kind = $derived(subject.kind);
  const id = $derived(subject.id);
  const schema = $derived(SCHEMAS[kind]);
  const info = $derived(
    (kind === "panel" ? screens.meta.panels : screens.meta.devices)?.find((x) => x.id === id) ?? null,
  );
  const displayName = $derived(info?.name ?? id);
  const users = $derived(configDraft.usage.panels?.[id]?.length ?? 0);
  const unseen = $derived(configDraft.usage.unseen?.length ?? 0);
  const dirtyCount = $derived((configDraft.revision, configDraft.dirtyCount(kind, id)));
  const pushBody = $derived((configDraft.revision, configDraft.pushBody(kind, id)));
  const unpushable = $derived((configDraft.revision, configDraft.unpushable(kind, id)));
  const live = $derived(configDraft.liveFor(kind, id));
  // Never show another subject's report (the store is shared and outlives a switch).
  const ownsResolution = $derived(resolution.subjectKey === subject.key);
  const ladderState = $derived(ownsResolution ? resolution.state : "none");
  // A device must keep its screen: an emptied one would be sent as clear:["screen"], which the server rejects.
  const screenMissing = $derived(kind === "device" && !plain("screen"));
  // A number field holding no number keeps its old value in the draft; Save
  // and Push wait until it is finished, so it can never travel as a removal.
  const invalid = $derived((configDraft.revision, configDraft.invalid(kind, id)));
  const canSave = $derived(dirtyCount > 0 && !screenMissing && invalid.length === 0);

  const cid = (rung, field) => `cfg-${rung}-${field.replaceAll(".", "-")}`;
  const val = (rung, field) => (configDraft.revision, configDraft.value(kind, id, rung, field));
  const plain = (field) => (configDraft.revision, configDraft.plainValue(kind, id, field));

  // A browser reports a half-typed number ("-", "1e") as `badInput` with an
  // empty `value`; NaN tells the store it is no number rather than a removal.
  const read = (t) => (t.validity?.badInput ? NaN : t.value);
  function onField(rung, field, e) {
    configDraft.set(kind, id, rung, field, read(e.currentTarget));
  }
  function onPlain(field, e) {
    const t = e.currentTarget;
    let v = read(t);
    if (field === "maximum_compatibility") v = v === "" ? null : v === "true";
    configDraft.setPlain(kind, id, field, v);
  }
  async function push() {
    if (Object.keys(pushBody).length === 0) return; // an empty push would store {} as a live override
    await configDraft.push(kind, id, { watchDevice: kind === "panel" ? subject.watchDevice : "" });
  }

  // Closing the Edit panel gives up a pushed override (spec §6); the draft stays.
  onDestroy(() => {
    configDraft.release();
  });
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col text-sm" data-testid="config-editor">
  {#snippet subjectName()}
    <span data-testid="edit-subject" class="truncate font-mono text-foreground">{kind} {displayName}</span>
    <span class="inline-flex shrink-0 items-center gap-1 text-muted-foreground">
      <Circle class="size-2 fill-current" aria-hidden="true" />{dirtyCount} unsaved
    </span>
  {/snippet}
  {#snippet headerActions()}
    <Button type="button" size="sm" variant="ghost" disabled={dirtyCount === 0 && !live} onclick={() => configDraft.discard(kind, id)}>Discard</Button>
    <Button type="button" size="sm" variant="outline" disabled={!canSave} onclick={() => canSave && configDraft.save(kind, id)}>Save</Button>
    <Button type="button" size="sm" disabled={Object.keys(pushBody).length === 0 || invalid.length > 0} onclick={push}>
      {kind === "panel" ? "Push to panel" : "Push to device"}
    </Button>
  {/snippet}
  <PaneHeader data-testid="config-actions" role="toolbar" aria-label="Actions"
              title="Edit" subject={subjectName} actions={headerActions} />
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3" data-testid="config-body">
  {#if invalid.length}
    <p role="alert" class="text-xs text-destructive">not a number: {invalid.join(", ")} — finish or clear it to Save.</p>
  {/if}
  {#if unpushable.length}
    <p class="text-xs text-muted-foreground">applies on Save only: {unpushable.join(", ")}</p>
  {/if}

  {#if kind === "panel"}
    <p class="text-xs">{users === 1 ? "⚠ 1 device uses this panel" : `⚠ ${users} devices use this panel`}</p>
    {#if unseen > 0}<p class="text-xs text-muted-foreground">{unseen === 1 ? "1 more device has not polled since byonk started, so its panel is not known yet." : `${unseen} more devices have not polled since byonk started, so their panel is not known yet.`}</p>{/if}
  {/if}

  {#if live}
    <div role="status" class="my-2 rounded border border-destructive p-2 text-xs">
      <p>◉ Live on {displayName} — the real {kind === "panel" ? "panel" : "display"} is showing this draft.</p>
      <p>It reverts on its own within 30 minutes, or on Discard.</p>
    </div>
  {/if}

  {#if kind === "device"}
    <section class="flex flex-col gap-2 py-2" aria-label="Device">
      <Label for="cfg-plain-screen">Screen</Label>
      <select id="cfg-plain-screen" class={selectClass} value={plain("screen")} onchange={(e) => onPlain("screen", e)}>
        {#each screens.list as s (s.ref)}<option value={s.ref}>{s.ref}</option>{/each}
      </select>
      {#if screenMissing}<p role="alert" class="text-xs text-destructive">A device needs a screen — pick one to enable Save.</p>{/if}
      <Label for="cfg-plain-name">Name</Label>
      <Input id="cfg-plain-name" value={plain("name") ?? ""} oninput={(e) => onPlain("name", e)} />
      <Label for="cfg-plain-params">Params (JSON)</Label>
      <textarea id="cfg-plain-params" class={selectClass + " h-20 py-1.5 font-mono"}
                value={plain("params")} oninput={(e) => onPlain("params", e)}></textarea>
      <Label for="cfg-plain-temperature_profile">Temperature profile</Label>
      <select id="cfg-plain-temperature_profile" class={selectClass} value={plain("temperature_profile") ?? ""}
              onchange={(e) => onPlain("temperature_profile", e)}>
        <option value="">(unset)</option><option value="default">default</option>
        <option value="a">a</option><option value="b">b</option>
      </select>
      <Label for="cfg-plain-maximum_compatibility">Maximum compatibility</Label>
      <select id="cfg-plain-maximum_compatibility" class={selectClass}
              value={plain("maximum_compatibility") === null ? "" : String(plain("maximum_compatibility"))}
              onchange={(e) => onPlain("maximum_compatibility", e)}>
        <option value="">(unset)</option><option value="true">on</option><option value="false">off</option>
      </select>
      <Label for="cfg-plain-min_png_bytes">Minimum PNG bytes</Label>
      <Input id="cfg-plain-min_png_bytes" type="number" min="0" value={plain("min_png_bytes") ?? ""}
             oninput={(e) => onPlain("min_png_bytes", e)} />
    </section>
  {/if}

  {#each schema.fields as field (field)}
    <section class="flex flex-col gap-1 border-t border-border py-2" aria-label={field}>
      <h3 class="font-mono text-xs">{field}</h3>
      {#each schema.rungs as rung (rung)}
        <Label for={cid(rung, field)}>{field} ({RUNG_LABEL[rung]})</Label>
        {#if field === "dither"}
          <select id={cid(rung, field)} class={selectClass} value={val(rung, field) ?? ""}
                  onchange={(e) => onField(rung, field, e)}>
            <option value="">— (none)</option>
            {#each screens.meta.dither_algorithms ?? [] as a (a.name)}<option value={a.name}>{a.name}</option>{/each}
          </select>
        {:else if field === "colors_actual" && kind === "panel"}
          <Input id={cid(rung, field)} value={val(rung, field) ?? ""} oninput={(e) => onField(rung, field, e)} />
          <SwatchEditor value={val(rung, field) ?? info?.colors ?? ""}
                        baseline={info?.colors_actual ?? info?.colors ?? ""}
                        onchange={(v) => configDraft.set(kind, id, rung, field, v)} />
        {:else if field === "colors" || field === "colors_actual"}
          <Input id={cid(rung, field)} value={val(rung, field) ?? ""} oninput={(e) => onField(rung, field, e)} />
        {:else}
          <Input id={cid(rung, field)} type="number" step={STEP[field]} value={val(rung, field) ?? ""}
                 oninput={(e) => onField(rung, field, e)} />
        {/if}
      {/each}
      <Ladder setting={field} editing={schema.rungs} state={ladderState} failure={ownsResolution ? resolution.failure : null}
              stale={!layout.isOpen("preview")}
              report={ladderState === "ok" ? resolution.setting(field) : null} />
    </section>
  {/each}

  </div>
</div>
