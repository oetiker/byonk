#!/usr/bin/env bash
# Render every bundled screen for the three-state comparison around the
# resvg byonk-base integration. Runs against any checkout, because it drives
# only the `byonk render` CLI.
#
# Bucket assignment (det vs nondet) was settled by an actual repeat-run
# diff (Task 1, Step 4), not guessed: `examples/mandelbrot` seeds its RNG
# from `time_now()` and is NOT reproducible between runs, so it lives in
# nondet despite looking like a pure computation. `EXAMPLES_DIR` is set
# below so the `examples` screen repo (mandelbrot, demo/font/*, hello,
# gphoto, webscrape, swiss-departure-board) auto-registers — it is not yet
# a registered screen repo in `config.yaml` on this branch.
#
# Two checks guard against the manifest looking like coverage it doesn't
# have (Task 1 review, round 1 + round 2): a CLI whose "screen not found"
# path falls back to the DEFAULT device's screen instead of erroring would
# make every render below exit 0 while actually capturing the wrong
# content — which is exactly what happened when this harness was first run
# against the pre-#30 `main` worktree (10 of 13 screens silently got the
# fallback splash).
#
#   1. CANARY (mechanism check): render a deliberately nonexistent screen
#      ref first, and record which way it went. Its purpose is to keep the
#      answer current in every manifest rather than assumed — and the answer
#      HAS CHANGED once already, which is the point of measuring it.
#
#      When this harness was written, `run_script_for_device` in
#      `src/services/content_pipeline.rs` let a *registered* device whose
#      `screen:` ref failed to resolve fall through to whatever DEFAULT
#      pointed at, so the canary exited 0 and `exit=0` below proved only
#      that *something* rendered. Commit 3a35030 ("refuse a device whose
#      configured screen does not resolve") removed that fallback, so on the
#      current tree the canary exits NON-ZERO and `exit=0` below does mean
#      "this screen rendered". Do not tighten either branch into an
#      assertion: the manifest states which behaviour was live for that run,
#      and a future reader needs that either way.
#   2. DISTINCTNESS (symptom check): every deterministic screen's PNG must
#      differ from every other deterministic screen's PNG. Now that the
#      canary shows the fallback is gone, this is belt-and-braces rather
#      than the primary guard — keep it, because it costs nothing and is
#      the only check that would catch two screens rendering the same
#      content for some reason other than a fallback. `tools/capture-
#      config.yaml`'s reserved `DEFAULT` device deliberately points at
#      `byonk-builtin/calibration/grey` — one of the deterministic screens
#      this harness already captures byte-for-byte — specifically so a
#      fallback isn't just "different from its own six siblings" (which a
#      single silently-broken ref would sail through unnoticed) but
#      collides byte-for-byte with `calibration-grey.png` (round 1's design
#      compared det screens only to each other, which misses exactly the
#      single-screen-falls-back case; round 2 fixes that by making the
#      fallback target a screen already in the comparison set). This is
#      sound with no clock dependency, unlike diffing against
#      `builtin-default` (which shows the current time and differs
#      run-to-run even between two correct renders) — `calibration/grey` has
#      no clock in it. Known residual gap: the fallback's dithering uses the
#      *calling* device's own panel (see `main.rs`'s `device_config` ->
#      `panel` chain), not DEFAULT's, so `calibration-color` (the one
#      deterministic screen on `trmnl_og_4clr` instead of `trmnl_og`) would
#      fall back to grey content dithered through the 4-color palette —
#      different bytes from `calibration-grey.png` (dithered through
#      `trmnl_og`), so a broken `calibration-color` ref alone would NOT be
#      caught by this check. It would still be visually obvious (a grey
#      test-pattern dithered into red/yellow bands) to the human eye a later
#      task hands differing screens to.
#
# Both verdicts are written into MANIFEST.txt, because the manifest is the
# artifact a human reads later to decide what was actually covered.
#
# STDERR IS CAPTURED, NOT DISCARDED. `byonk render` writes authoring warnings
# (currently the render-scale warning) to stderr and nowhere else, so an
# earlier version of this script that sent stderr to /dev/null would have
# reported a clean capture for a screen byonk was actively complaining about.
# Per-screen stderr lands in `<name>.stderr` next to the PNG, and any non-empty
# one is called out in MANIFEST.txt.
#
# BYONK_BIN selects the binary. Default is `cargo run --release` for a
# from-scratch checkout; export BYONK_BIN=./target/debug/byonk to reuse an
# existing debug build, which is seconds instead of minutes. rust-embed reads
# screens from disk in a debug build, so a debug capture reflects the working
# tree, not the last build.
set -uo pipefail

OUT="${1:?usage: capture-renders.sh <output-dir>}"
CFG="${CONFIG_FILE:-tools/capture-config.yaml}"
EXAMPLES="${EXAMPLES_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/screens/examples}"
mkdir -p "$OUT/nondeterministic"

# Word-split deliberately: the default is a multi-word command.
# shellcheck disable=SC2206
BYONK=(${BYONK_BIN:-cargo run --release --quiet --})

CANARY_MAC="AA:BB:CC:00:00:99"

# mac:name:bucket   bucket = det | nondet
SCREENS="
AA:BB:CC:00:00:01:calibration-gamut:det
AA:BB:CC:00:00:02:calibration-tone:det
AA:BB:CC:00:00:03:calibration-grey:det
AA:BB:CC:00:00:04:calibration-color:det
AA:BB:CC:00:00:06:demo-font-bitmap:det
AA:BB:CC:00:00:07:demo-font-ttf:det
AA:BB:CC:00:00:08:demo-font-hinting:det
AA:BB:CC:00:00:21:calibration-grey-16grey:det
AA:BB:CC:00:00:22:calibration-tone-16grey:det
AA:BB:CC:00:00:23:calibration-color-6clr:det
AA:BB:CC:00:00:24:calibration-gamut-6clr:det
AA:BB:CC:00:00:25:calibration-tone-6clr:det
AA:BB:CC:00:00:26:calibration-gamut-6clr-13in:det
AA:BB:CC:00:00:05:mandelbrot:nondet
AA:BB:CC:00:00:11:hello:nondet
AA:BB:CC:00:00:12:builtin-default:nondet
AA:BB:CC:00:00:13:gphoto:nondet
AA:BB:CC:00:00:14:webscrape:nondet
AA:BB:CC:00:00:15:swiss-departure-board:nondet
"

: > "$OUT/MANIFEST.txt"

# --- 1. Canary: prove an unresolved screen ref still hard-errors ----------
CONFIG_FILE="$CFG" EXAMPLES_DIR="$EXAMPLES" "${BYONK[@]}" \
    render --mac "$CANARY_MAC" --output "$OUT/.canary.png" >/dev/null 2>&1
canary_exit=$?
rm -f "$OUT/.canary.png"
if [ "$canary_exit" -eq 0 ]; then
  {
    echo "CANARY: fallback-to-DEFAULT is active (unresolved screen ref exited 0, as expected"
    echo "  on the current codebase — see the header comment). Every exit=0 below is NOT"
    echo "  proof the requested screen rendered; it may be DEFAULT's fallback screen instead."
    echo "  The DISTINCTNESS check below is what actually guards this run — read it first."
  } >> "$OUT/MANIFEST.txt"
else
  {
    echo "CANARY: unresolved screen ref now hard-errors (exit=$canary_exit) instead of"
    echo "  falling back — the fallback-to-DEFAULT behavior this harness was built around"
    echo "  has changed. Re-check the header comment and capture-config.yaml's mac/screen"
    echo "  map before trusting exit=0 below as coverage."
  } >> "$OUT/MANIFEST.txt"
fi

# --- 2. Render every screen -------------------------------------------------
for entry in $SCREENS; do
  mac="${entry%:*:*}"
  rest="${entry#"$mac":}"
  name="${rest%:*}"
  bucket="${rest##*:}"
  [ "$bucket" = det ] && dir="$OUT" || dir="$OUT/nondeterministic"
  CONFIG_FILE="$CFG" EXAMPLES_DIR="$EXAMPLES" "${BYONK[@]}" \
      render --mac "$mac" --output "$dir/$name.png" \
      >/dev/null 2>"$dir/$name.stderr"
  echo "$name $bucket exit=$?" >> "$OUT/MANIFEST.txt"
done

# --- 2b. Surface anything byonk wrote to stderr ----------------------------
# `byonk render` puts authoring warnings on stderr and nowhere else, so a
# capture that discards them reports "13 screens rendered" for a tree byonk is
# warning about. Any non-empty stderr is quoted into the manifest verbatim.
warn_found=0
for f in "$OUT"/*.stderr "$OUT"/nondeterministic/*.stderr; do
  [ -s "$f" ] || continue
  warn_found=1
  {
    echo "STDERR from $(basename "${f%.stderr}"):"
    sed 's/^/  | /' "$f"
  } >> "$OUT/MANIFEST.txt"
done
[ "$warn_found" -eq 0 ] && echo "STDERR: every screen rendered silently — byonk emitted no warnings" >> "$OUT/MANIFEST.txt"

# --- 3. Distinctness: no two deterministic renders may be identical -------
# DEFAULT points at calibration-grey (see capture-config.yaml), so a
# fallback lands byte-identical to calibration-grey.png regardless of
# whether one screen fell back or several — this is what actually catches
# gap 1 from round 2's review (a single mis-resolved screen has no other
# sibling to collide with; it collides with the fallback target itself,
# which is already in this comparison set).
dup_found=0
det_files=("$OUT"/*.png)
for ((i = 0; i < ${#det_files[@]}; i++)); do
  for ((j = i + 1; j < ${#det_files[@]}; j++)); do
    if cmp -s "${det_files[$i]}" "${det_files[$j]}"; then
      echo "DISTINCTNESS FAIL: $(basename "${det_files[$i]}") == $(basename "${det_files[$j]}") — one of these likely fell back to DEFAULT (calibration/grey) instead of rendering its own screen" >> "$OUT/MANIFEST.txt"
      dup_found=1
    fi
  done
done
[ "$dup_found" -eq 0 ] && echo "DISTINCTNESS ok: no deterministic screen collided with another, including the DEFAULT fallback target (calibration-grey.png) — see the header comment for the one panel-dependent case this can't catch (calibration-color)" >> "$OUT/MANIFEST.txt"

cat "$OUT/MANIFEST.txt"
