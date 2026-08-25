# Handover — PR #43 is open; the next job is the cache-key collision

**Date:** 2026-08-25 · **Branch:** `feat/panel-clean-recovery` · **HEAD:** `ff2267f`
**Base:** `main` @ `5c67c62` (v0.19.0, protected). `main` is an ancestor; no rebase needed.
**PR:** [#43](https://github.com/oetiker/byonk/pull/43) — open, +6655/−662 across 45 files.

> **`cargo test --workspace` is GREEN.** The previous handover said one test was
> red on purpose; that test is deleted and the whole suite passes. Any failure
> now is a regression.

## Resume here

**Start with §1.** The owner has ruled: the cache-key collision is a follow-up,
tackled this session, *not* folded into PR #43. Everything else below is context.

1. **§1 — the task.** Cache-key collision. Fully scoped, start immediately.
2. **§2 — PR #43.** What is in it, and the review feedback already handled.
3. **§3 — the dot-gain investigation.** Conclusion reversed since the last
   handover. Read this before touching the ditherer.
4. **§4 — the box.** What is live, and what to put back.
5. **§5 — measurement kit.** Patterns generated, analyses self-tested.
6. **§6 — defects carried forward.**
7. **§7 — environment.** Traps that cost time.

---

## 1. THE TASK: the content cache is keyed on the SVG alone

Raised by Copilot on PR #43 against `min_png_bytes`. The diagnosis was too
narrow — **this is pre-existing on `main` and affects every render parameter**,
not just the new one.

### The defect

`src/services/content_cache.rs`:

```rust
pub fn new(rendered_svg: String, screen_name: String, width: u32, height: u32) -> Self {
    let content_hash = compute_svg_hash(&rendered_svg);   // <-- SVG only
    ...
}
```

`compute_svg_hash` hashes the SVG and nothing else, and it runs in the
**constructor** — before any `with_*` builder has been applied. So none of these
are in the cache key, though all of them change the bytes served:

`colors` · `colors_actual` · `dither` · `max_error` · `noise_scale` ·
`chroma_clamp` · `strength` · `gamut` · `font_hinting` · `min_png_bytes`

The cache is a global `HashMap<String, CachedContent>` and `store()` does a plain
`insert`, so the last writer wins. Two devices that render byte-identical SVG —
same model, same screen, same data — but differ in any per-device render setting
will share one entry, and whichever rendered last decides what both of them get
from `/api/image/{hash}.png`.

Narrow in practice, silent when it happens.

### Why the shape of the fix matters

Folding in only `min_png_bytes` would look like a fix and leave `colors` and
`dither` equally broken. Key on **every render-affecting input**.

The obstacle is ordering: the hash is computed in the constructor, the parameters
arrive afterwards through the builder chain. Good news — all four call sites
already read `content_hash` *after* the full chain:

| call site | what it renders |
|---|---|
| `src/api/display.rs:649` | registration screen |
| `src/api/display.rs:1056` | the normal content path |
| `src/api/display.rs:1081` | the normal content path |
| `src/api/display.rs:1108` | error screen |

Each does `let hash = cached.content_hash.clone();` after building, so moving the
hash to a terminal step is a small, mechanical change. Two options:

- **`finish()` that recomputes** and returns `Self`. Simplest. Risk: someone
  forgets to call it and gets a stale hash silently.
- **A distinct `CachedContentBuilder` type** whose `build()` produces
  `CachedContent`. The type system enforces it. More code, no footgun.

Prefer the second unless it fights the existing style.

### Two things to decide before writing code

1. **Every content hash changes**, so every device refetches once on upgrade.
   Harmless but real; it belongs in `CHANGES.md`.
2. `gamut` and `font_hinting` are structs, not scalars — they need a stable
   serialisation for hashing. Derive it, do not hand-roll field concatenation, or
   the next field added silently drops out of the key.

**Write the failing test first**: two `CachedContent` values from the same SVG
with different `min_png_bytes` must not collide. That test fails on `main` today,
which is the proof the defect is pre-existing.

---

## 2. PR #43 — open, review feedback handled

Thirty-seven commits, four independent pieces of work. **No quantiser code
reaches `main`** — it was built, measured and removed inside the branch, and the
net diff contains zero lines of it.

| what | where |
|---|---|
| Panel recovery for burnt-in screens | `services/recovery.rs`, `api/admin/recovery.rs` |
| TRMNL X ghosting levers + PNG padding | `rendering/png_pad.rs`, `models/config.rs` |
| **Data-loss fix**: device patch ate other settings | `api/admin/write.rs` (`0fb5c47`) |
| Error screen ran off the panel | `services/template_service.rs` |
| `error_clamp` → `max_error` (breaking) | crate-wide (`3351151`) |
| Device, not panel, decides the algorithm (breaking) | (`a20f2ee`) |
| Ink Field calibration screen | `screens/builtin/calibration/inkfield/` |

### The three review comments

1. **`dev.js` stale sentinels — real, fixed in `ff2267f`.** The UI populates
   `max_error`/`noise_scale` from the per-algorithm `DITHER_DEFAULTS`, then
   compared them against a fixed `'0.08'`/`'5'`. Neither matches any current
   default, so **every** dev render sent both as overrides the user never asked
   for, pinning the server's defaults to whatever the UI showed. Now compares
   against the selected algorithm's defaults, numerically so `1.0` and `1` agree.
2. **localStorage `errorClamp` → `maxError` migration — declined, on purpose.**
   In 0.18.0 the knob stopped capping the pixel value and started capping the
   accumulated error, moving its range from ~0.1 to ~1.0 while keeping its name.
   That is *why* it was renamed, and the server deliberately discards
   `error_clamp` rather than migrating it. A saved value carries no date, so
   migrating one risks reviving a number chosen for the old meaning. The UI now
   **logs** what it dropped and why — the "silently" part of the objection was
   fair, the migration was not.
3. **Cache key — deferred to §1**, with a reply on the thread explaining that it
   is pre-existing and broader than the comment claims.

### Before merging

- The two breaking changes want a human eye; they change existing configs.
- `CHANGES.md` is covered (+111) and never mentions the quantiser.
- The withdrawn quantiser spec and plan are **kept**, banner-marked in `e785619`.
  The plan needed it most: it opened by instructing agentic workers to implement
  it, ahead of 1,564 lines of steps for deleted code.

---

## 3. Dot gain — the conclusion reversed since the last handover

> The previous handover's §1 said the mechanism is geometric: "the darker state
> expands into the lighter one." **That is refuted by its own data.** Do not
> build on it.

Full write-up: `~/scratch/panel-evidence/dotgain-2026-08-23/FINDINGS-dotgain.md`.

### Why geometric is dead

A geometric effect is driven by **boundary length**, which at 25% coverage with
s×s clusters is exactly `1/s`. So the excess must quarter from 1px to 4px.
Measured, `excess(1)/excess(4)` is **1.09, 1.14, 0.81** — flat. It should be 4.00.

Confirmed independently by macro photography: **colour dots on black are the
correct physical size** yet photometer at 0.13 against a nominal 0.25. Correct
size with a wrong reading is the optical signature — the dots are not growing,
the light is being redistributed.

A follow-up claim that yellow dots on blue and green looked wrong-sized **was
withdrawn** on closer inspection. Blue-on-green and green-on-blue barely register
by eye, but that is the inks: **they are the closest pair this panel has**, 0.2788
apart in OKLab against 0.9223 for yellow-to-black.

### What it means for the ditherer — do not build this yet

Error diffusion already guarantees that the area-average of its output, *in the
space it computes error in*, matches the target. That space is linear RGB, which
assumes linear mixing. Under Yule–Nielsen the panel mixes linearly in `R^(1/n)`,
so mapping palette **and** image through `R → R^(1/n)` puts the guarantee on the
apparent colour. One exponent, no neighbour bookkeeping, `n = 1` a bit-for-bit
no-op.

Simulated against the real ditherer: mean ΔE **0.0602 → 0.0086 at n=1.45**. Green
on neutrals goes **down**, 21.7% → 19.2%.

**Blocked on two unmeasured numbers.** The mis-specification penalty is roughly
symmetric — correcting for n=1.45 beats doing nothing only if the truth is ≥1.25,
and costs **5× if the panel is linear**. And the whole effect scales with the
panel's **black reflectance**, which nobody has measured; `colors_actual` declares
`#000000`, a rendering convention, not a reflectance. Across 0%→10% of white the
value of the fix falls 70×.

**Cheapest decisive step: photometer the panel's solid black and solid white.**
Two patches. That ratio decides whether any of it is worth building.

---

## 4. The box — `root@10.46.18.3`, ssh authorised by the owner

| | |
|---|---|
| `43664941_byonk` | **0.19.0, started** — the one serving |
| `local_byonk` | 0.19.0-mix2, stopped |
| device `44:1B:F6:83:93:38` | E1004, `floyd-steinberg`, screen **`local/calibration/dotpairs1a`** |
| device `94:A9:90:8C:6D:18` | TRMNL Classic, `jarvis-judice-ninke`, unchanged |

**Restore the E1004 to `examples/gphoto` when the dot-gain photography is done.**

Live config, in **both** `config.yaml` files so it survives whichever add-on runs
(backups `config.yaml.pre-kernel-2026-08-23`, `config.yaml.pre-green-2026-08-23`):
kernel `atkinson-hybrid` → **`floyd-steinberg`**, panel green `#1E5645` →
**`#00994D`**.

> **Do NOT replace `#00994D` with the photographic `#1E5645`.** Told green is
> nearly neutral, the ditherer picks it for every dark neutral and paints skin
> shadows green. This one line took the panel from "extreme green tint" to "much
> better".

**Screens on the box, not in the repo** (`/addon_configs/43664941_byonk/screens/calibration/`,
handle `local`): `dotgain`, `dotgain2a`, `dotgain2b`, `dotpairs1a`, `dotpairs1b`,
`dotpairs3a`, `dotpairs3b`.

**MCP cannot write binary files** — `pattern.png` goes over `scp`. Verify with
`sha256sum` on both ends.

The pattern screens are a **pre-built indexed PNG placed 1:1**; every source pixel
is already a palette entry, so the ditherer returns it unchanged with zero error.
**Verified end to end for `dotpairs1a`: 1,920,000 of 1,920,000 pixels identical
and all 30 cells at exactly 0.1250**, by pulling back the PNG the panel actually
fetched. Worth re-checking after any pattern edit — these carry antialiased text,
which the ditherer *does* dither.

---

## 5. Measurement kit — `~/scratch/panel-evidence/dotgain-2026-08-23/`

| file | what |
|---|---|
| `FINDINGS-dotgain.md` | **the write-up** — mechanism, simulation, decision table |
| `scripts/make_dotpairs.py` | all 30 ordered ink pairs, 1px and 3px, 12.5% |
| `scripts/measure_pairs.py` | the verdict; `--selftest` discriminates **both** ways |
| `scripts/make_dotgain3.py` | coverage sweep × cluster size, per-row references |
| `scripts/fit_dotgain3.py` | fits optical *and* geometric + the glare offset |
| `scripts/pair_conditioning.py` | which pairs can carry an answer at all |
| `scripts/measure2.py`, `locate_grid.py`, `find_panel.py` | the v2 chain |
| `patterns/` | `dotgain2a/b`, `dotgain3a/b`, `dotpairs1a/1b/3a/3b` |

### Two traps this kit already fell into

- **Half the pair matrix cannot carry a photometric answer.** The estimator
  divides by `|ink − bg|²`, and 7 of 15 pairs are worse conditioned than
  yellow-on-white, the cell v2 discarded for exactly that reason (worst:
  black/green 0.107 vs 0.959). **Raising coverage does not help** — the coverage
  error is `ε/|d|`, with no coverage term in it. Read the 1px/3px ratio only on
  pairs involving white or yellow; black-on-white alone settles the mechanism.
- **A plausible exponent is not evidence of the mechanism.** Data generated from
  a purely geometric law still fits an optical `n = 1.387`. Only the residual
  ratio separates them. Never quote an `n` without its competing fit.

Raw workflow: `dcraw -4 -T -o 1 -w -q 3 -b N`. `-4` is linear — pick `N` per frame
so nothing clips. One camera module per frame set, fixed ISO.

---

## 6. Defects carried forward

1. **`colors_actual` serves two masters** — an honest preview and the ditherer's
   ink choice. §3 makes this sharper: they are not the same number, and **black
   is where they diverge**. An appearance model needs a true reflectance; the
   preview needs something monitor-plausible.
2. **The dev UI duplicates `DitherAlgorithm::defaults()`** (`static/dev/dev.js`,
   `DITHER_DEFAULTS`). `ff2267f` fixed the symptom; the duplication remains and
   will drift again. Fetch them from the server.
3. **A panel's tuned dither parameters silently do nothing when the device picks
   a different algorithm.** `dither:` is keyed by algorithm name.
   `deprecation_warnings()` is the place to report from.
4. **`noise_scale: 5`** in the shipped E1002/E1004 blocks; the measured optimum
   for Sierra Lite is 2.5 (`dither/mod.rs:166`).
5. **MCP cannot set a device's dither algorithm.** `assign_screen` takes only
   `mac` and `screen_ref`, while `apply_device_patch` (`write.rs:249`) already
   accepts `dither`, `panel`, `refresh`, `colors`, `params`, `name`. Widen it,
   rename to `configure_device`. Felt repeatedly — every kernel change needs ssh.
6. **Panels have no write path at all** — no `/panels` route in `admin_router()`,
   REST or MCP. The whole calibration workflow requires ssh.
7. **`docs/src/concepts/content-pipeline.md:261`** — stale `error_clamp` range.
   The owner's file, uncommitted; the owner fixes it.
8. **`sierra-lite` deserves one re-test** with a sane `max_error` and the
   `sierra-light` block cleaned up. Note `sierra-light` is a deliberate alias
   (`config.rs:214`), so the E1004's existing block is *armed*, not dead.
9. **Re-run the nine-kernel ranking on the corrected green.** Floyd–Steinberg
   beating Atkinson is structural and safe, but the order among the seven
   full-error kernels was measured with the too-dull green.

From TRMNL X: the palette rejects duplicate colours, and `map_grey_indices`
derives the level from an entry's **index** rather than its hex, so "declare only
the usable inks" does not work.

---

## 7. Environment

**The owner's four files are permanently modified and must never be staged:**
`config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`,
`tools/capture-config.yaml`. **Never `git add -A` or `git add .`** — add by
explicit path and check `git diff --cached --name-only` before every commit.

- **Verify with the four commands directly. `make check` has reported exit 0
  while tests failed.**
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd docs && mdbook build
  ```
- **`git` needs `--no-pager`**, or `diff --stat`/`log` can return nothing and look
  like a clean tree.
- **A command reporting a permission denial may still have run.** A `git rm` came
  back denied by the auto-mode classifier and had in fact taken effect. Check the
  resulting state rather than trusting the error.
- **`git log <ref>..HEAD | wc -l` prints `0` for a nonexistent ref**, which reads
  identically to "up to date". Check the ref exists first.
- **Strip ANSI escapes before grepping add-on logs** — `content_hash=` is really
  `content_hash\e[0m\e[2m=\e[0m`, so the obvious pattern silently matches nothing.
  Pipe through `sed 's/\x1b\[[0-9;]*m//g'`.
- byonk **does not hot-reload config — restart the add-on.** A restart clears the
  render cache, so `/api/image/<hash>.png` 404s until the device polls. Screens
  *are* re-read per render.
- **`/api/image/<hash>.png` needs no token** and serves the exact PNG the panel
  received — the cheapest way to verify a render. **Filter the log by
  `width=1200`** or you will grab the Classic's 800×480 image.
- MCP `render_screen` accepts `dither`, `panel`, `colors_actual`. **Pass explicit
  `width`/`height`** — `image_max_width` resamples and destroys the dither pattern.
- The box has **`jq` but no `python3`**. The Mac has **PIL but no numpy or rawpy**.
- No `timeout` on this Mac and foreground `sleep` is blocked; loop with `sleep`
  inside a remote `ssh`, or use `curl --retry`.
- Devices: `44:1B:F6:83:93:38` reTerminal E1004 (1200×1600, 6-colour) and
  `94:A9:90:8C:6D:18` TRMNL Classic.

---

## 8. Older material, still true

- **Photographic calibration of the E1004** —
  `~/scratch/panel-evidence/e1004-2026-08-21/FINDINGS.md`. Read it with §3: its
  green is not usable for the ditherer, and a solid-patch method cannot predict
  dithered output. Veiling glare is the biggest error source and its offset was
  not identifiable from the Latin square — **`fit_dotgain3.py` now fits it as a
  free parameter**, which is what a coverage sweep buys.
- **Never mix camera modules in a frame set.** One telephoto frame at ISO 500
  among ISO 64 main-camera frames was a blue-channel outlier by up to 9.2% of
  white.
- **TRMNL X** (full text in `814d061`): two firmware grey tables selected by PNG
  byte size (100 KiB), so `colors_actual` belongs to *(panel, grey table)*, not to
  the panel. Pin with `min_png_bytes`. Restoring `homeio`: `log_level` → `info`,
  stop `local_byonk`, start `43664941_byonk`, delete `local/noise-test`.
