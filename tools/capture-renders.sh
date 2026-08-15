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
# have (Task 1 review, round 1): a CLI whose "screen not found" path falls
# back to the DEFAULT splash instead of erroring would make every render
# below exit 0 while actually capturing the wrong content — which is
# exactly what happened when this harness was first run against the
# pre-#30 `main` worktree (10 of 13 screens silently got the DEFAULT
# splash). Byte-diffing against a `builtin-default` capture from the same
# run does NOT catch this reliably, because `builtin-default` itself shows
# a clock and differs run-to-run. Instead:
#
#   1. CANARY (mechanism check): render a deliberately nonexistent screen
#      ref first. Verified against `src/services/content_pipeline.rs`
#      (`run_script_for_device`): a *registered* device whose `screen:` ref
#      fails to resolve falls through to the DEFAULT device's screen (which
#      always resolves, so nothing ever reaches the CLI's error path) — the
#      canary is therefore EXPECTED to exit 0 on every run against the
#      current tree, not just on a regression. Its purpose is to make that
#      fact loud and current in every manifest, rather than assumed, since
#      it is exactly what made 10/13 of the state-1 (pre-#30) renders look
#      like coverage without being it. A canary that ever starts exiting
#      non-zero would mean the fallback path was removed or DEFAULT itself
#      stopped resolving — also worth knowing.
#   2. DISTINCTNESS (symptom check): every deterministic screen's PNG must
#      differ from every other deterministic screen's PNG. Two identical
#      bytes among clock-free renders means at least one of them rendered
#      the DEFAULT fallback (or some other shared content) instead of its
#      own screen — this is the check that actually catches a mis-resolved
#      screen in a given run, since the canary alone cannot.
#
# Both verdicts are written into MANIFEST.txt, because the manifest is the
# artifact a human reads later to decide what was actually covered.
set -uo pipefail

OUT="${1:?usage: capture-renders.sh <output-dir>}"
CFG="${CONFIG_FILE:-tools/capture-config.yaml}"
EXAMPLES="${EXAMPLES_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/screens/examples}"
mkdir -p "$OUT/nondeterministic"

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
AA:BB:CC:00:00:05:mandelbrot:nondet
AA:BB:CC:00:00:11:hello:nondet
AA:BB:CC:00:00:12:builtin-default:nondet
AA:BB:CC:00:00:13:gphoto:nondet
AA:BB:CC:00:00:14:webscrape:nondet
AA:BB:CC:00:00:15:swiss-departure-board:nondet
"

: > "$OUT/MANIFEST.txt"

# --- 1. Canary: prove an unresolved screen ref still hard-errors ----------
CONFIG_FILE="$CFG" EXAMPLES_DIR="$EXAMPLES" cargo run --release --quiet -- \
    render --mac "$CANARY_MAC" --output "$OUT/.canary.png" >/dev/null 2>&1
canary_exit=$?
rm -f "$OUT/.canary.png"
if [ "$canary_exit" -eq 0 ]; then
  {
    echo "CANARY: fallback-to-DEFAULT is active (unresolved screen ref exited 0, as expected"
    echo "  on the current codebase — see the header comment). Every exit=0 below is NOT"
    echo "  proof the requested screen rendered; it may be the DEFAULT splash instead."
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
  CONFIG_FILE="$CFG" EXAMPLES_DIR="$EXAMPLES" cargo run --release --quiet -- \
      render --mac "$mac" --output "$dir/$name.png" >/dev/null 2>&1
  echo "$name $bucket exit=$?" >> "$OUT/MANIFEST.txt"
done

# --- 3. Distinctness: no two deterministic renders may be identical -------
dup_found=0
det_files=("$OUT"/*.png)
for ((i = 0; i < ${#det_files[@]}; i++)); do
  for ((j = i + 1; j < ${#det_files[@]}; j++)); do
    if cmp -s "${det_files[$i]}" "${det_files[$j]}"; then
      echo "DISTINCTNESS FAIL: $(basename "${det_files[$i]}") == $(basename "${det_files[$j]}")" >> "$OUT/MANIFEST.txt"
      dup_found=1
    fi
  done
done
[ "$dup_found" -eq 0 ] && echo "DISTINCTNESS ok: all deterministic renders differ from each other" >> "$OUT/MANIFEST.txt"

cat "$OUT/MANIFEST.txt"
