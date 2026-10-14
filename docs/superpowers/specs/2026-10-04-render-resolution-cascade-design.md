# Render Resolution Cascade — Design

Date: 2026-10-04 (revised same day after review)
Status: design, not yet planned
Scope: server only. No dev-UI change. Prerequisite for
`2026-10-04-dev-ui-work-area-design.md`.

> **Revision note.** The first draft of this spec contained three factual
> errors about the existing code, all found by review and all verified before
> correcting: the `refresh` precedence was stated backwards (§2), the claim
> that every render path resolves the dither algorithm before its per-algorithm
> tuning was false for two of five paths (§4.2), and the device write surface
> was described as "additive" when it has seven coupled sites and a
> silent-overwrite trap (§4.8). The cascade also had four rungs where the
> running code has six sources (§2). Nothing is removed from the historical
> record — this file *is* the record, and it was never implemented from.

## 1. Problem

byonk resolves each render setting through its own precedence ladder, and the
ladders do not agree with each other.

| setting | today, highest first | resolved in |
|---|---|---|
| `colors` | script → device → panel → firmware `Colors` header → `#000000,#555555,#AAAAAA,#FFFFFF` | `resolve_render_params` (`display.rs:396-404`), fallback at `display.rs:992` |
| `colors_actual` | script → dev override → panel → `Measured-Colors` header | `resolve_measured_colors` (`display.rs:284-316`) |
| tuning numbers | script → device → panel-per-algorithm → engine algorithm default | `resolve_tuning`, `resolve_effective_tuning` (`display.rs:166-181`) |
| `refresh` | **script `refresh_rate`** → device → screen `meta.yaml` | `resolve_refresh_rate` (`content_pipeline.rs:27-50`) |
| **`dither` algorithm** | **device → script → `atkinson`** | `display.rs:411-427`, winner chosen at `:427` |

Four ladders put the screen above the device. The fifth inverts it. The
inversion is deliberate and documented (`display.rs:362-368`): "the device
outranks the screen because the algorithm is a property of the panel, not of
the content, and the operator who set it on the device cannot see a screen
replacing it."

That reasoning is sound; the remedy is not. It gives the device owner a veto
over exactly one setting, and gives it unconditionally — so an owner who merely
wants a *default* for `dither` cannot express that, and an owner who wants a
veto over anything else cannot have one. When the two disagree byonk drops the
screen's value and pushes a warning into `warnings` (`display.rs:417-425`),
which is the design admitting it cannot tell a default from a veto.

Two gaps follow from the same cause:

- **`colors_actual` has no device level at all.** `DeviceConfig` carries
  `colors` but not `colors_actual`, so a measurement belonging to one physical
  unit rather than to every unit of its type cannot be recorded.
- **`gamut` is unreachable through the write API.** It is absent from
  `DeviceWrite` (`write.rs:22-55`) and survives only by accident, carried
  across verbatim by `unmanaged_device_keys`.

## 2. The cascade

One ladder, applied uniformly. Five rungs and a per-setting fallback:

```
override  →  device final  →  screen  →  device base  →  panel  →  fallback
highest                                                            lowest
```

| rung | means | who sets it |
|---|---|---|
| `override` | a dev-UI experiment in flight — live, temporary, never config | the dev UI, explicitly |
| `device final` | "whatever the screen says, on *this* display, no" | device owner |
| `screen` | a decision made with the image in hand | screen author, from the script return |
| `device base` | "normally use this on this display" | device owner |
| `panel` | what this hardware *is* — shared by every device of the type | calibration |
| *fallback* | not a rung: what applies when the ladder is empty | byonk, or the device's own firmware |

Resolution is **field by field**, highest non-`None` wins — with one documented
exception for `colors_actual` (§4.4).

### Why this ordering

The screen sits above the device's default because the script is the only
participant that can inspect the content it is about to render and choose
accordingly: a photograph and a timetable want different dithering, and only
the script knows which it just produced. The device owner sits above the screen
in `final` because the owner is the only participant who can see the physical
result, and a screen author cannot know that this particular display hangs in a
dark hallway.

Giving the owner *two* rungs is what lets intent be stated rather than
inferred. Today's single device rung is read as a veto for `dither` and as a
default for everything else, which is why neither reading is ever right.

`override` sits on top because a dev-UI experiment must be able to show what
*any* rung would do, including one it is shadowing. It was absent from the
first draft of this spec, which would have made the dev UI's ladder widget
report the config winner while a Push was live — wrong in exactly the two modes
where Push exists (see `2026-10-04-dev-ui-work-area-design.md` §5).

### Rungs versus fallbacks

A **fallback** is what applies when no rung carries a value. It is not
something anyone configures per device, so it is not a rung — and this is where
the device's own firmware headers belong, because a hardware self-report is
evidence, not policy.

| setting | fallback chain, below the panel rung |
|---|---|
| `colors` | firmware `Colors` header → `#000000,#555555,#AAAAAA,#FFFFFF` |
| `colors_actual` | `Measured-Colors` header → none (render with nominal colours) |
| `dither` | `atkinson` |
| `max_error`, `noise_scale`, `chroma_clamp`, `strength`, `gamut` | the engine's per-algorithm defaults |
| `refresh` | the screen's `meta.yaml` `refresh` |

### Fields in the cascade

`colors`, `colors_actual`, `dither`, `max_error`, `noise_scale`,
`chroma_clamp`, `strength`, `gamut`, `refresh`.

Every one is already settable by the screen (`lua_runtime.rs:25-44`,
`content_pipeline.rs:68-83`, plus `refresh_rate` in the script return), so the
screen rung needs no new surface — only a consistent position.

### `refresh` is in, at `base` — and that is a zero-behaviour-change move

The first draft excluded `refresh` on the stated grounds that "the device
currently wins, so it already behaves like `final`". **That was backwards.**
`resolve_refresh_rate` (`content_pipeline.rs:34-40`) is
`if lua_refresh > 0 { lua_refresh } else if device_override { … } else {
screen_default }` — the **script wins**, and the displaced device value is
recorded as `ignored_device_override` specifically so the operator can be told
it is inert. `docs/src/api/lua-api.md:1283-1287` says so outright: *"The screen
outranks the device here — the opposite of `dither`."*

The draft also named the wrong screen rung. The screen rung is the script's
`refresh_rate` **return value**; `meta.yaml`'s `refresh` is the *fallback*,
below everything.

So `refresh` today already behaves exactly like `base`. Bringing it in with
existing `devices.<key>.refresh` values staying at `base` is therefore a **zero
behaviour change**, and it adds one genuinely useful capability: `final.refresh`,
an owner's veto on how often a battery-powered display wakes up. Had the draft's
migration instruction ("place existing values in `final`") been followed, every
device whose screen sets its own `refresh_rate` would have silently changed
cadence.

The existing `ignored_device_override` log line becomes redundant once the
ladder widget shows the same fact; it should stay until the widget ships.

### Fields deliberately NOT in the cascade

`screen`, `panel`, `name`, `params`, `temperature_profile`,
`maximum_compatibility`, `min_png_bytes`. No screen rung sits above any of
them, so a second device rung would resolve against nothing.

## 3. Config shape

Bare keys are the base rung. A `final:` block carries the veto.

```yaml
panels:
  trmnl_x:
    name: TRMNL X
    colors: "#000000,#FFFFFF,…"
    colors_actual: "#1E5645,#00994D,…"
    dither:                        # PanelDitherConfig, unchanged
      defaults: { max_error: 1.0 }
      algorithms:
        floyd-steinberg: { noise_scale: 0.6 }

devices:
  AA:BB:CC:DD:EE:FF:
    screen: local/hello            # outside the cascade
    panel: trmnl_x
    name: Kitchen
    params: { city: Olten }

    dither: floyd-steinberg        # base — a screen may override
    strength: 0.9                  # base
    refresh: 900                   # base — unchanged semantics (§2)
    colors_actual: "#1C5240,…"     # base — NEW at device level
    gamut: { knee: 0.8 }           # base — NEW to the write API (§4.8)

    final:
      dither: atkinson             # nothing overrides this
      refresh: 1800                # the battery veto
```

```rust
/// The settings that take part in the resolution cascade.
///
/// Used for the `final:` block and as the shape the resolver works in. One
/// struct and one resolver is what stops a newly added setting from quietly
/// acquiring a precedence order of its own, which is how `dither` came to be
/// inverted relative to its four neighbours.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CascadeValues {
    pub colors: Option<String>,
    pub colors_actual: Option<String>,
    pub dither: Option<String>,
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    pub refresh: Option<u32>,
    #[serde(default)]
    pub gamut: GamutTuningValues,
}

pub struct DeviceConfig {
    // …the non-cascade fields, unchanged…

    // The base rung stays ENUMERATED here rather than `#[serde(flatten)]`
    // into a CascadeValues — see below. Read as a rung through `base()`.
    pub colors: Option<String>,
    pub colors_actual: Option<String>,   // new
    pub dither: Option<String>,
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    pub refresh: Option<u32>,
    #[serde(default)]
    pub gamut: GamutTuningValues,

    /// The final rung. `final` is a reserved word in Rust, hence the rename.
    #[serde(rename = "final", default)]
    pub final_rung: Option<CascadeValues>,
}

impl DeviceConfig {
    /// The base rung as a `CascadeValues`, for the resolver.
    pub fn base(&self) -> CascadeValues { /* field copy */ }
}
```

### Why the base rung is not `#[serde(flatten)]`

Flatten is the obvious way to write this and it must not be used. Measured
directly on serde 1.0.228 + serde_yaml 0.9.34 (`Cargo.lock:4124,4202`), a
typo'd value reports:

```
enumerated:  devices.AA.strength: invalid type: string "oops", expected f32 at line 5 column 15
flattened:   devices.AA: invalid type: string "oops", expected f32 at line 3 column 5
```

`AppConfig::load_from_assets` (`config.rs:505-528`) documents that "the error
message includes the line and column of the first problem found", and surfaces
it as the startup failure `Invalid config.yaml: {e}`. In add-on mode that
message is the only thing the operator sees. §2 assigns the base rung to the
device owner and the panel rung to whoever calibrates, so this file is
hand-edited by design — losing the field name and the line number is a real
regression, and flatten buys nothing the resolver does not already give.

`deny_unknown_fields` on `CascadeValues` is therefore asymmetric: it guards the
`final:` block (so `final: {screen: …}` is a config error, not a silent drop)
but not the base rung, which keeps today's tolerant behaviour. That asymmetry
is deliberate; making the base rung strict is a separate, larger decision about
every existing `config.yaml`.

### Migration

Breaking, and acceptable: 0.20.0 already carries breaking changes and byonk has
no userbase. Exactly one behaviour changes:

> A bare `devices.<key>.dither` no longer beats a screen that sets its own.
> Move it to `final:` to keep the old behaviour.

`refresh` is unchanged (§2). `final:`, device `colors_actual` and device
`gamut` are additive. This goes in CHANGES.md as user-visible and breaking, and
it must name the Home Assistant consequence from §4.8.

## 4. Implementation

### 4.1 One resolver, five call sites

```rust
/// Where a resolved value came from.
///
/// `Fallback` is reported but never configured: §2's fallbacks are byonk's own
/// defaults and the device's self-reported headers, so nothing writes them per
/// device. It is a variant here because the provenance payload must be able to
/// say "nothing on the ladder was set, and this is what applied instead".
pub enum Rung { Fallback, Panel, DeviceBase, Screen, DeviceFinal, Override }

/// Every value a setting could take, lowest rung first.
pub struct Ladder<T> {
    pub fallback: Option<T>,
    pub panel: Option<T>,
    pub device_base: Option<T>,
    pub screen: Option<T>,
    pub device_final: Option<T>,
    pub over: Option<T>,
}
```

`resolve_render_params`, `resolve_effective_tuning`, `resolve_tuning`,
`resolve_measured_colors` and `resolve_refresh_rate` collapse into one
resolution over `CascadeValues`.

**All five call sites, and each one's `override` source.** The first draft
listed four and missed `admin/preview.rs` entirely:

| path | call site | `override` rung comes from |
|---|---|---|
| production `/api/display` | `display.rs:1055` (+ the inline tuning copy at `:1048`) | `DevOverrides.dither`/`.tuning` (device-keyed), `.panel_colors` (panel-keyed), and the new panel-keyed cascade map (§4.6) |
| dev render | `dev.rs:863` | the draft in the POST body (§4.6) |
| authoring + device preview | `screen_store.rs:1409` | `RenderOpts` (`opts.dither`, `opts.max_error`, …) |
| CLI render | `main.rs:450` | the CLI's own flags (`dither`, `cli_max_error`, `cli_strength`) |
| **admin device preview** | `admin/preview.rs:126` builds `DevicePreview` field-by-field; `:244` hashes those fields into the preview-cache fingerprint | same as `/api/display` |

`admin/preview.rs` matters twice. If `DevicePreview` does not gain `final` and
`colors_actual`, the admin preview renders a *different image* from
`/api/display` for the same device. And if `fingerprint()` does not hash them,
the preview cache **serves a stale image after a `final:` change** — silently,
until the TTL. That function's own doc comment says this class of bug "only
shows up on the glass hours later, which is exactly how it was found."

**Two paths derive the effective algorithm inconsistently today, and must be
fixed as part of this work.** The first draft asserted all of them were
correct; they are not:

- **`main.rs:429-432`** — `script_dither.or(dc_dither)`, i.e. **script first**,
  while `resolve_render_params` at `main.rs:450` renders `device.or(script)`. So
  the CLI looks up per-algorithm panel tuning for the *script's* algorithm while
  rendering with the *device's*.
- **`screen_store.rs:1377`** — `dither_override.or(effective_script_dither)`:
  the device rung is **absent**, while the rendered algorithm at
  `screen_store.rs:1420` is `dither_override.or(device_dither)` then script. A
  device preview with `devices.<k>.dither` set and a script that sets none
  renders with the device's algorithm but takes the panel's *default* tuning
  block rather than that algorithm's.

These are live wrong-pixel bugs independent of the cascade. They must be named
as work items, because §7's convergence test fails on them, and an implementer
who believes §4.2's old claim would weaken the test instead of fixing the code.

### 4.2 Algorithm first, then its numbers

`PanelDitherConfig` holds per-algorithm tuning, so the numbers can only be
resolved once the algorithm is known. `/api/display` does this correctly
(`display.rs:1022-1036`, `panel_final_tuning`, with a comment naming the
hazard); `dev.rs:816-823` does too. The other two are §4.1's broken pair. The
two-pass shape must hold in **all five** paths after this work, and it matters
more than before because the screen now wins `dither` more often.

Per-algorithm tuning stays a panel-only refinement. Device base, device final
and override take flat values; per-algorithm blocks on a device would multiply
the config surface by the algorithm count for a requirement nobody has stated.

### 4.3 The conflict warning is deleted

`display.rs:417-425` pushes "screen asked for dither X but the device is
configured for Y; the device wins" into `warnings`. With `final` available, a
device/screen disagreement is the cascade working as specified. The message and
its plumbing for this case go away.

### 4.4 `colors_actual` resolves with a validity rule

§2's "highest non-`None` wins" is **not** true for `colors_actual`, and the
first draft claimed it was. `resolve_measured_colors` (`display.rs:284-316`)
walks the chain and a candidate that *is* supplied but whose entry count ≠ the
resolved palette length is **discarded with a warning and the walk continues**.
Its doc comment at `:271-283` is explicit: "A candidate that IS supplied but
whose length doesn't match `palette_len` is discarded, not fatal… the first
candidate whose length matches wins outright."

That rule is correct and stays. It means the resolver returns, per rung, not
just a value but **what happened to it**:

```rust
pub enum RungStatus {
    Winner,
    Outranked,               // a higher rung carried a value
    Rejected { reason: String },  // supplied, but unusable — e.g. wrong entry count
    Absent,
}
```

Every setting uses the same type; only `colors_actual` can currently produce
`Rejected`. This is not cosmetic: it is the difference between the dev UI
saying "a rung above yours wins" and "your six-entry calibration was thrown
away because this render has a sixteen-entry palette" — in the one mode where
each attempt costs a minute of panel repaint.

### 4.5 Provenance

The dev UI needs every rung's value *and* status. Returned as a response header
on the **dev** render only:

```
X-Byonk-Resolution: {"dither":{"effective":"atkinson","from":"screen","rungs":{"fallback":{"value":"atkinson","status":"outranked"},"panel":{"value":"floyd-steinberg","status":"outranked"},"device_base":{"status":"absent"},"screen":{"value":"atkinson","status":"winner"},"device_final":{"status":"absent"},"override":{"status":"absent"}}}, …}
```

Header values cannot contain newlines; the example above is wrapped for
reading only.

**Size: budget ~2 KB, not the 600 bytes the first draft estimated.** A
sixteen-entry `colors` string is ~135 characters, and `colors` plus
`colors_actual` across five rungs and `effective` is ~1.35 KB before the other
seven settings. Still far inside header limits, but nothing should be sized on
600 bytes.

A header rather than a sibling endpoint, because the screen rung only exists
after the script has run: a separate endpoint would re-run the script, pay
twice, and still be able to disagree with the image on screen. Production
`/api/display` does not carry it — no consumer, and the bytes go to a
battery-powered device.

### 4.6 The dev render becomes a POST that carries the draft

`RenderQuery` (`dev.rs:30-68`) is a flat GET query with one slot per setting,
no `gamut`, and **no way to say which rung a value belongs to**. The cascade
adds a rung dimension, and the dev UI's whole try-before-save story depends on
previewing an uncommitted two-rung draft. There is no transport for that today.

`POST /dev/api/render`, replacing the query-parameter override set:

```json
{
  "screen": "local/hello",
  "device": "AA:BB:CC:DD:EE:FF",
  "panel": "trmnl_x",
  "draft": { "base": { … }, "final": { … } },
  "simulated": { "battery_voltage": 4.1, "rssi": -67, "timestamp": 0, "params": {} }
}
```

Answers the PNG plus `X-Byonk-Resolution`. The `draft` feeds the `override`
rung, which is what makes a preview show an uncommitted change without
touching server state or hardware.

**`screen` and `device` must be accepted together.** Today `dev.rs:488-520` is
strictly either/or: when `mac` is present, `query.screen` *and* `query.panel`
are discarded and everything comes from the device entry. That makes the dev
UI's "render *this* screen as *that* device" unbuildable
(`2026-10-04-dev-ui-work-area-design.md` §4, mode a). The POST takes the screen
from `screen` and every render setting from `device`.

### 4.7 Overrides need a TTL, and a panel-keyed cascade map

Two gaps in `DevOverrides` (`server.rs:40-52`):

- **`panel_colors` holds one `colors_actual` string per panel.** There is no
  panel-keyed map for the other cascade fields, so "push the whole panel draft"
  has nowhere to live. Add a panel-keyed `CascadeValues` map and read it as the
  `override` rung for every panel-rung field, not just measured colours.
- **None of the override maps expires.** Only `tuning_sessions` has a TTL
  (`server.rs:66`, lazily dropped at `:69-73`), and that governs the refresh
  *rate*, not the override. Release is `onDestroy`-only in the UI, which does
  not run on tab close, crash or navigation — so a physical panel can keep
  rendering an uncommitted draft indefinitely. Give the override maps the same
  lazy TTL, so the dev UI's "it reverts on its own" is true rather than
  reassuring.

### 4.8 The write surface is not additive: seven coupled sites

The first draft said `final` support is "additive in all three writers". It is
not, and the most likely implementation fails *silently*.

**The trap.** `MANAGED_DEVICE_KEYS` (`write.rs:280`) lists what `device_block`
writes. Anything not in it is carried across verbatim by
`unmanaged_device_keys` (`write.rs:328-346`) and re-inserted **after**
`device_block` (`write.rs:570-573`). So if `final` is not added to that list,
**every PATCH that sets or clears `final` is overwritten by the `final` block
already in the file**, with no error. `gamut` is in exactly this state today,
which is why §1 lists it as unreachable.

Every site that must change together:

1. `DeviceWrite` (`write.rs:22-55`) — add nested `final`, plus base
   `colors_actual` **and `gamut`**.
2. `MANAGED_DEVICE_KEYS` — add `final`, `colors_actual`, `gamut`.
3. `CLEARABLE_DEVICE_KEYS` — add the same three. An invariant test
   (`write.rs:969-980`) asserts this list equals `MANAGED_DEVICE_KEYS` minus
   `screen`, so adding to one without the other fails there; the test is the
   reason this coupling is survivable at all.
4. `device_block` (`write.rs:280+`) — emit the `final` mapping.
5. The merge struct (`write.rs:541-561`) — see the ruling below.
6. `resolve_clear`'s also-set table (`write.rs:220-235`).
7. `apply_clear` (`write.rs:252-270`).

Plus the invariant test itself, extended to cover the dotted form.

**Ruling — `final` merges field by field, like every other rung.** §5.3
condemns whole-block replacement for tuning; the same argument applies here.
Setting `final.dither` must not wipe `final.strength`.

**Ruling — `clear` takes one dot.** A bare name clears the **base rung only**;
it must never clear both.

```
clear: ["dither"]         → base.dither
clear: ["final.dither"]   → final.dither
clear: ["final"]          → the whole final rung
```

Field-by-field merge means a final-rung field can only be removed through
`clear`, which is why both the dotted and whole-rung forms are needed.

**The MCP tool does not inherit.** `PATCH /api/admin/devices/{key}` does, since
it takes `Json<DeviceWrite>`. But `ConfigureDeviceArgs` (`mcp/tools_device.rs:19-73`)
is a **hand-written mirror** of `DeviceWrite` with its own `JsonSchema` and its
own field-by-field conversion in the `body()` closure at `:126-143`. `final`,
`colors_actual` and `gamut` must be added there explicitly. `gamut` is already
missing there, demonstrating this exact drift.

**Home Assistant: the Dither select must move to `final.dither`.**
`custom_components/byonk/select.py:80-87` reads `device.get("dither")` and
writes `{"dither": option}` — the base rung. Today that write is a veto, so
after the flip a user picks "atkinson" in Home Assistant, the entity *shows*
"atkinson", and the device renders whatever the screen asked for. That is a
Home Assistant entity reporting a value that is not in effect — the same
"cannot tell a default from a veto" failure this spec exists to remove,
relocated into HA. The HA control has always meant an owner's veto, so moving
it to `final.dither` preserves its meaning and keeps existing HA users at zero
behaviour change. `config_flow.py:193-207` sets `dither` at onboarding the same
way and moves with it. The first draft's "the HA integration needs no change to
keep working" was a decision made on a wrong premise.

`require_writable_global` is not involved. A device mapping is not global
config (`write.rs:486-488`) and `panels` is absent from the add-on Options
schema (`dev.rs:380-382`), so both rungs stay writable in add-on mode.

## 5. Defects

**5.1 and 5.2 are FIXED** — commit `cd01ce9b`, with
`tests/dev_render_is_read_only_test.rs`. Recorded here because they motivated
the "preview never writes" rule and because 5.2's diagnosis belongs with the
cascade.

- **5.1 — `GET /dev/api/render` wrote to real hardware.** `dev.rs:767-811`
  synced the query's dither/tuning/colours into `DevOverrides` keyed by
  `query.mac`, so `/api/display` served them to the physical device. Worse than
  first described: the `else` branches called `remove()`, so a preview that
  omitted those fields **cleared an override someone set on purpose**. Added
  deliberately in `bb9437ea` and shipped in v0.12.0–v0.19.0.
- **5.2 — a dead write in a mismatched key space.** The same block inserted
  into `panel_colors` by **device** key while `display.rs:842` reads it by
  **panel** key. Inert unless the key spaces collide, and a silent cross-device
  repaint when they do, which is how the regression test pins it.

**5.3 is OPEN, and it is not dev-only** — the first draft said all three
defects were "all in the dev render path", which is wrong.
`resolve_effective_tuning` (`display.rs:166-181`) replaces the **whole** block
when the override sets any field, so overriding `strength` alone silently drops
the panel's `max_error`. It is called from `dev.rs:860` and
`screen_store.rs:1400` — and `/api/display` carries its own inline copy of the
same logic at **`display.rs:1048-1052`**. Scoped as "dev only", the fix lands in
one place and the copy that reaches real hardware survives. The `Ladder`
resolver is field-by-field, which fixes it — in all five paths.

**5.4 — the authoring preview cannot override `strength`.**
`screen_store.rs:1390` passes `strength: None` into `opts_tuning` while
carrying the other three fields. Verified; cause not diagnosed. Fix or document
while in there.

## 6. Documentation

Unowned in the first draft, and three shipped pages become actively false. A
screen author reading them would conclude their `dither` is inert and remove
it.

- `docs/src/api/lua-api.md:1364-1377` — the numbered dither chain ("1. Dev UI
  override 2. Device config 3. Script 4. Default") and "**The device outranks
  the screen here.**" Every sentence changes.
- `docs/src/api/lua-api.md:1283-1287` — the `refresh` chain. Still correct
  under §2's decision; confirm rather than rewrite.
- `docs/src/api/lua-api.md:1288-1300` — the `colors` chain, including the
  firmware `Colors` header rung. Reframe as §2's rungs plus fallback.
- `docs/src/guide/configuration.md:519-540` — both priority tables, and the
  "the algorithm suits the panel rather than the content" rationale.
- `docs/src/concepts/content-pipeline.md:253,268` — two priority chains.
  ⚠ **This file is permanently modified in the maintainer's working tree.**
  Coordinate before editing; never `git add` it as a side effect.
- `docs/src/guide/configuration.md` — a new section for `final:`, device
  `colors_actual` and device `gamut`.

## 7. Testing

- **A resolution table test.** One test per setting × per rung, asserting the
  value, the winning rung, and each other rung's `RungStatus`. The table is the
  specification of §2; a setting that forgets a rung fails here.
- **`dither` flipping, named explicitly**: screen beats device base; device
  final beats screen; override beats everything.
- **`refresh` is unchanged**: a script's `refresh_rate` still beats a bare
  `devices.<k>.refresh`; `final.refresh` beats the script. This is the test
  that would have caught the first draft's inverted migration.
- **Field-by-field, not block-by-block**: a rung setting one field leaves the
  others to the rungs below (regression test for 5.3), asserted on
  `/api/display` as well as the dev path.
- **`colors_actual`'s validity rule**: a wrong-length value on a middle rung is
  reported `Rejected` and the walk continues to a lower rung.
- **Algorithm-then-numbers, per path**: a screen that changes the algorithm
  gets that algorithm's per-algorithm panel tuning. One test per path, because
  this is per-path logic — and two of them fail today (§4.1).
- **The five call paths agree.** One fixture resolved through `/api/display`,
  `POST /dev/api/render`, `ScreenStore::render`, the CLI and
  `/api/admin/devices/{key}/preview` must produce identical `RenderParams`.
  The highest-value test in this spec. **It will fail on §4.1's two broken
  orderings before they are fixed — that is the test working, not the test
  being wrong.**
- **The preview cache fingerprint covers `final` and `colors_actual`**: change
  only `final:` and assert the cached preview is invalidated.
- **`clear` addressing**: `["dither"]` clears base and leaves final;
  `["final.dither"]` the reverse; `["final"]` the whole rung. Plus the
  `MANAGED`/`CLEARABLE` invariant test extended for the dotted form.
- **A PATCH setting `final` survives `unmanaged_device_keys`** — the §4.8 trap,
  which fails silently and so needs its own test.
- **The MCP `configure_device` tool can set `final`, `colors_actual` and
  `gamut`** — it is a hand-written mirror, so only a test keeps it in step.
- **Override TTL**: an override past its TTL stops affecting `/api/display`.

## 8. Scope: three shippable pieces, in order

This is not one piece of work.

1. **`dev.rs` safety (defects 5.1 + 5.2).** Needs none of the cascade.
   **DONE — `cd01ce9b`.**
2. **The provenance payload** (§4.4 status type, §4.5 header, §4.6 POST
   render). Additive and read-only apart from the endpoint reshape, and it is
   the *only* hard dependency of the dev-UI spec. Shipping it before the write
   surface unblocks the UI work.
3. **The cascade itself** (§2, §3, §4.1–4.3, §4.7, §4.8, defects 5.3/5.4) plus
   §6's documentation. Genuinely interdependent: the five paths must flip
   together.

§4.1's two broken algorithm orderings are live bugs today. They can ship with
piece 3, or earlier on their own; they must not ship *later*, because piece 2's
convergence test depends on them.

## 9. Open questions

1. **Should the panel get a `final` rung too?** Not in this design. A panel's
   values are the floor, and a panel-level veto over a device's own base rung
   would make a per-device setting unreliable for reasons its owner cannot see.
2. **Should the base rung become strict (`deny_unknown_fields`)?** Out of scope
   here (§3). It would turn every typo in every existing `config.yaml` into a
   startup failure — worth doing, as its own decision.
3. **`refresh` in the ladder widget.** Once the widget shows the rung, is the
   `ignored_device_override` log line still worth keeping?
