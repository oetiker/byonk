<script>
  import { onDestroy, untrack } from "svelte";
  import * as api from "../api.js";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft, SCHEMAS } from "../state/configDraft.svelte.js";
  import { resolution } from "../state/resolution.svelte.js";
  import { layout } from "../state/layout.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import Ladder from "./Ladder.svelte";
  import SwatchEditor from "./SwatchEditor.svelte";
  import { Input } from "$lib/ui/input/index.js";
  import * as Field from "$lib/ui/field/index.js";
  import { NativeSelect } from "$lib/ui/native-select/index.js";
  import { Textarea } from "$lib/ui/textarea/index.js";
  import { Alert } from "$lib/ui/alert/index.js";
  import { Circle, Save, Undo2 } from "@lucide/svelte";
  import PaneHeader from "./PaneHeader.svelte";
  import HeaderAction from "./HeaderAction.svelte";

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
  const dirtyCount = $derived((configDraft.version, configDraft.dirtyCount(kind, id)));
  const pushBody = $derived((configDraft.version, configDraft.pushBody(kind, id)));
  const unpushable = $derived((configDraft.version, configDraft.unpushable(kind, id)));
  const live = $derived(configDraft.liveFor(kind, id));
  // Never show another subject's report (the store is shared and outlives a switch).
  const ownsResolution = $derived(resolution.subjectKey === subject.key);
  const ladderState = $derived(ownsResolution ? resolution.state : "none");
  // A device must keep its screen: an emptied one would be sent as clear:["screen"], which the server rejects.
  const screenMissing = $derived(kind === "device" && !plain("screen"));
  // A number field holding no number keeps its old value in the draft; Save
  // and Push wait until it is finished, so it can never travel as a removal.
  const invalid = $derived((configDraft.version, configDraft.invalid(kind, id)));
  // Identity Save cannot do without (an emptied Id, Name, Colors or Key).
  const missing = $derived((configDraft.version, configDraft.missing(kind, id)));
  const MISSING_LABEL = { id: "an id", name: "a name", colors: "colors", key: "a key" };
  const canSave = $derived(dirtyCount > 0 && !screenMissing && invalid.length === 0 && missing.length === 0);
  // A refused Save (409/400) keeps the draft; the server's reason shows here.
  const saveError = $derived((configDraft.version, configDraft.saveError(kind, id)));

  // Key suggestions: devices byonk has seen that no entry configures yet,
  // fetched when a device opens in the editor. A failure only means none.
  let pending = $state([]);
  $effect(() => {
    if (kind !== "device") return;
    id; // re-fetch per device
    untrack(() => {
      pending = [];
      api.listPendingDevices().then(
        (list) => (pending = Array.isArray(list) ? list : []),
        () => (pending = []),
      );
    });
  });

  const cid = (rung, field) => `cfg-${rung}-${field.replaceAll(".", "-")}`;
  const val = (rung, field) => (configDraft.version, configDraft.value(kind, id, rung, field));
  const plain = (field) => (configDraft.version, configDraft.plainValue(kind, id, field));

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
    <!-- In a narrow header (below 22rem, see HeaderAction) the word goes
         and the dot + count stay, so the subject keeps room. -->
    <span class="inline-flex shrink-0 items-center gap-1 text-muted-foreground">
      <Circle class="size-2 fill-current" aria-hidden="true" /><span class="@max-[22rem]:sr-only">{dirtyCount} unsaved</span><span aria-hidden="true" class="hidden @max-[22rem]:inline">{dirtyCount}</span>
    </span>
  {/snippet}
  {#snippet headerActions()}
    <HeaderAction icon={Undo2} label="Discard" variant="ghost" disabled={dirtyCount === 0 && !live} onclick={() => configDraft.discard(kind, id)} />
    <HeaderAction icon={Save} label="Save" disabled={!canSave} onclick={() => canSave && configDraft.save(kind, id)} />
    <HeaderAction
      label={kind === "panel" ? "Push to panel" : "Push to device"}
      short="Push"
      variant="default"
      disabled={Object.keys(pushBody).length === 0 || invalid.length > 0}
      onclick={push}
    />
  {/snippet}
  <PaneHeader data-testid="config-actions" role="toolbar" aria-label="Actions"
              title="Edit" subject={subjectName} actions={headerActions} />
  {#if invalid.length || unpushable.length || saveError}
    <!-- Fixed row under the header (not in the scrolling body): the Save/Push blockers stay visible. -->
    <div data-testid="config-messages" class="flex shrink-0 flex-col gap-0.5 border-b border-border px-2 py-1 text-xs">
      {#if saveError}
        <Field.Error class="text-xs">Save failed: {saveError}</Field.Error>
      {/if}
      {#if invalid.length}
        <Field.Error class="text-xs">not a number: {invalid.join(", ")} — finish or clear it to Save.</Field.Error>
      {/if}
      {#if unpushable.length}
        <p class="text-muted-foreground">applies on Save only: {unpushable.join(", ")}</p>
      {/if}
    </div>
  {/if}
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3" data-testid="config-body">
  <!-- Details (U7): what names the entry. Save-only: never pushed, never
       rendered until saved. -->
  <section id="config-details" class="flex flex-col gap-1 pb-2" aria-label="Details">
    <h3 class="font-mono text-xs">Details</h3>
    <Field.Group class="gap-3">
    {#if kind === "panel"}
      <Field.Field>
        <Field.Label for="cfg-plain-id">Id</Field.Label>
        <Input id="cfg-plain-id" class="font-mono" value={plain("id") ?? ""} oninput={(e) => onPlain("id", e)} />
      </Field.Field>
      <Field.Field>
        <Field.Label for="cfg-plain-name">Name</Field.Label>
        <Input id="cfg-plain-name" value={plain("name") ?? ""} oninput={(e) => onPlain("name", e)} />
      </Field.Field>
      <Field.Field>
        <Field.Label for="cfg-plain-match">Match</Field.Label>
        <Input id="cfg-plain-match" class="font-mono" placeholder="the firmware's Board header" value={plain("match") ?? ""}
               oninput={(e) => onPlain("match", e)} />
      </Field.Field>
      <div class="grid grid-cols-2 gap-3">
        <Field.Field>
          <Field.Label for="cfg-plain-width">Width</Field.Label>
          <Input id="cfg-plain-width" type="number" min="1" max="4096" step="1" value={plain("width") ?? ""}
                 oninput={(e) => onPlain("width", e)} />
        </Field.Field>
        <Field.Field>
          <Field.Label for="cfg-plain-height">Height</Field.Label>
          <Input id="cfg-plain-height" type="number" min="1" max="4096" step="1" value={plain("height") ?? ""}
                 oninput={(e) => onPlain("height", e)} />
        </Field.Field>
      </div>
      <Field.Field>
        <Field.Label for="cfg-plain-colors">Colors</Field.Label>
        <Input id="cfg-plain-colors" class="font-mono" value={plain("colors") ?? ""} oninput={(e) => onPlain("colors", e)} />
        <Field.Description class="text-xs">The official palette, comma-separated hex.</Field.Description>
      </Field.Field>
    {:else}
      <Field.Field>
        <Field.Label for="cfg-plain-key">Key</Field.Label>
        <Input id="cfg-plain-key" class="font-mono" list="cfg-pending-keys" value={plain("key") ?? ""}
               oninput={(e) => onPlain("key", e)} />
        <datalist id="cfg-pending-keys">
          {#each pending as p (p.mac)}<option value={p.mac}>{p.model}</option>{/each}
        </datalist>
      </Field.Field>
      <Field.Field>
        <Field.Label for="cfg-plain-name">Name</Field.Label>
        <Input id="cfg-plain-name" value={plain("name") ?? ""} oninput={(e) => onPlain("name", e)} />
      </Field.Field>
      <Field.Field>
        <Field.Label for="cfg-plain-screen">Screen</Field.Label>
        <NativeSelect id="cfg-plain-screen" value={plain("screen")} onchange={(e) => onPlain("screen", e)}>
          {#each screens.list as s (s.ref)}<option value={s.ref}>{s.ref}</option>{/each}
        </NativeSelect>
        {#if screenMissing}<Field.Error class="text-xs">A device needs a screen — pick one to enable Save.</Field.Error>{/if}
      </Field.Field>
      <Field.Field>
        <Field.Label for="cfg-plain-panel">Panel</Field.Label>
        <NativeSelect id="cfg-plain-panel" value={plain("panel") ?? ""} onchange={(e) => onPlain("panel", e)}>
          <option value="">auto (from the device's Board header)</option>
          {#each screens.meta.panels ?? [] as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </NativeSelect>
      </Field.Field>
    {/if}
    {#each missing as f (f)}
      <Field.Error class="text-xs">A {kind} needs {MISSING_LABEL[f]} — fill it in to enable Save.</Field.Error>
    {/each}
    </Field.Group>
  </section>

  {#if kind === "panel"}
    <p class="text-xs">{users === 1 ? "⚠ 1 device uses this panel" : `⚠ ${users} devices use this panel`}</p>
    {#if unseen > 0}<p class="text-xs text-muted-foreground">{unseen === 1 ? "1 more device has not polled since byonk started, so its panel is not known yet." : `${unseen} more devices have not polled since byonk started, so their panel is not known yet.`}</p>{/if}
  {/if}

  {#if live}
    <Alert variant="destructive" role="status" class="my-2 text-xs">
      <p>◉ Live on {displayName} — the real {kind === "panel" ? "panel" : "display"} is showing this draft.</p>
      <p>It reverts on its own within 30 minutes, or on Discard.</p>
    </Alert>
  {/if}

  {#if kind === "device"}
    <section class="border-t border-border py-2" aria-label="Device">
      <Field.Group class="gap-3">
        <Field.Field>
          <Field.Label for="cfg-plain-params">Params (JSON)</Field.Label>
          <Textarea id="cfg-plain-params" class="max-h-40 min-h-20 overflow-auto font-mono"
                    value={plain("params")} oninput={(e) => onPlain("params", e)} />
        </Field.Field>
        <Field.Field>
          <Field.Label for="cfg-plain-temperature_profile">Temperature profile</Field.Label>
          <NativeSelect id="cfg-plain-temperature_profile" value={plain("temperature_profile") ?? ""}
                        onchange={(e) => onPlain("temperature_profile", e)}>
            <option value="">(unset)</option><option value="default">default</option>
            <option value="a">a</option><option value="b">b</option>
          </NativeSelect>
        </Field.Field>
        <Field.Field>
          <Field.Label for="cfg-plain-maximum_compatibility">Maximum compatibility</Field.Label>
          <NativeSelect id="cfg-plain-maximum_compatibility"
                        value={plain("maximum_compatibility") === null ? "" : String(plain("maximum_compatibility"))}
                        onchange={(e) => onPlain("maximum_compatibility", e)}>
            <option value="">(unset)</option><option value="true">on</option><option value="false">off</option>
          </NativeSelect>
        </Field.Field>
        <Field.Field>
          <Field.Label for="cfg-plain-min_png_bytes">Minimum PNG bytes</Field.Label>
          <Input id="cfg-plain-min_png_bytes" type="number" min="0" value={plain("min_png_bytes") ?? ""}
                 oninput={(e) => onPlain("min_png_bytes", e)} />
        </Field.Field>
      </Field.Group>
    </section>
  {/if}

  {#each schema.fields as field (field)}
    <section class="flex flex-col gap-1 border-t border-border py-2" aria-label={field}>
      <h3 class="font-mono text-xs">{field}</h3>
      <Field.Group class="gap-2">
      {#each schema.rungs as rung (rung)}
        <Field.Field>
        <Field.Label for={cid(rung, field)}>{field} ({RUNG_LABEL[rung]})</Field.Label>
        {#if field === "dither"}
          <NativeSelect id={cid(rung, field)} value={val(rung, field) ?? ""}
                        onchange={(e) => onField(rung, field, e)}>
            <option value="">— (none)</option>
            {#each screens.meta.dither_algorithms ?? [] as a (a.name)}<option value={a.name}>{a.name}</option>{/each}
          </NativeSelect>
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
        </Field.Field>
      {/each}
      </Field.Group>
      <Ladder setting={field} editing={schema.rungs} state={ladderState} failure={ownsResolution ? resolution.failure : null}
              stale={!layout.isOpen("preview")}
              report={ladderState === "ok" ? resolution.setting(field) : null} />
    </section>
  {/each}

  </div>
</div>
