#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
fail=0

mkscreen() { # dir, byonk-value
  mkdir -p "$tmp/screens/$1"
  cat > "$tmp/screens/$1/meta.yaml" <<EOF
title: A screen
description: Something
byonk: "$2"
refresh: 900
params:
  byonk: not-the-engine-field
EOF
}

check() { # file, expected byonk line, label
  local got
  got=$(grep '^byonk:' "$tmp/screens/$1/meta.yaml")
  if [ "$got" = "$2" ]; then
    echo "  ok: $3"
  else
    echo "  FAIL: $3 -- expected '$2', got '$got'"; fail=1
  fi
}

echo "feature release 0.17.1 -> 0.18.0"
mkscreen builtin/default 0.17
mkscreen examples/hello 0.17
mkscreen examples/ahead 0.18
mkdir -p "$tmp/docs/src/tutorial"
cat > "$tmp/docs/src/tutorial/first-screen.md" <<'EOF'
# First screen

```yaml
title: Hello
byonk: "0.17"       # engine series (caret range)
```

Prose mentioning byonk: "0.17" stays put.
EOF
"$here/bump-screen-engine.sh" "0.18.0" "$tmp" >/dev/null
check builtin/default 'byonk: "0.18"' "stale screen moved to the new series"
check examples/hello  'byonk: "0.18"' "every screen in the tree is visited"
check examples/ahead  'byonk: "0.18"' "an already-current screen is left correct"

echo "the nested key under params: is not the engine field"
if grep -q '^  byonk: not-the-engine-field$' "$tmp/screens/builtin/default/meta.yaml"; then
  echo "  ok: indented byonk: untouched"
else
  echo "  FAIL: the rewrite reached an indented byonk: key"; fail=1
fi

echo "the tutorial's meta.yaml example moves with the screens"
check_doc() { # expected, label
  local got; got=$(grep '^byonk:' "$tmp/docs/src/tutorial/first-screen.md")
  if [ "$got" = "$1" ]; then echo "  ok: $2"; else echo "  FAIL: $2 -- expected '$1', got '$got'"; fail=1; fi
}
check_doc 'byonk: "0.18"       # engine series (caret range)' "fenced yaml example bumped, trailing comment kept"
if grep -q '^Prose mentioning byonk: "0.17" stays put.$' "$tmp/docs/src/tutorial/first-screen.md"; then
  echo "  ok: a mid-line mention in prose is untouched"
else
  echo "  FAIL: the rewrite reached mid-line prose"; fail=1
fi
# Pins the limitation the script documents rather than leaving it to a comment:
# this is a column-0 match, not a fenced-block parse, so a prose line that starts
# like a YAML key IS rewritten. If that ever stops being true, update both.
printf '%s\n' 'byonk: "0.17" written at column 0 outside any fence.' \
  >> "$tmp/docs/src/tutorial/first-screen.md"
"$here/bump-screen-engine.sh" "0.18.0" "$tmp" >/dev/null
if grep -q '^byonk: "0.18" written at column 0 outside any fence.$' "$tmp/docs/src/tutorial/first-screen.md"; then
  echo "  ok: column-0 prose is rewritten too (documented limitation)"
else
  echo "  FAIL: the documented column-0 behaviour changed"; fail=1
fi

echo "a quote in the trailing comment is not swallowed"
mkdir -p "$tmp/screens/quoted"
printf '%s\n' 'title: Q' 'byonk: "0.17"  # see the "compat" section' > "$tmp/screens/quoted/meta.yaml"
"$here/bump-screen-engine.sh" "0.18.0" "$tmp" >/dev/null
got=$(grep '^byonk:' "$tmp/screens/quoted/meta.yaml")
if [ "$got" = 'byonk: "0.18"  # see the "compat" section' ]; then
  echo "  ok: comment preserved"
else
  echo "  FAIL: expected the comment intact, got '$got'"; fail=1
fi

echo "idempotent: running it again changes nothing"
# Sorted, so the digest cannot depend on find's unspecified traversal order, and
# over the .md files too -- the script rewrites those, so leaving them out would
# let a docs-only regression pass. shasum rather than sha256sum: macOS ships no
# sha256sum, and this suite has to run on a maintainer's laptop as well as CI.
digest() {
  find "$tmp" \( -name meta.yaml -o -name '*.md' \) -type f | sort \
    | while IFS= read -r f; do cat "$f"; done | shasum
}
sum_before=$(digest)
"$here/bump-screen-engine.sh" "0.18.0" "$tmp" >/dev/null
sum_after=$(digest)
if [ "$sum_before" = "$sum_after" ]; then echo "  ok: no change on a second run"; else echo "  FAIL: not idempotent"; fail=1; fi

echo "bugfix release 0.18.0 -> 0.18.1 stays in the same series"
"$here/bump-screen-engine.sh" "0.18.1" "$tmp" >/dev/null
check builtin/default 'byonk: "0.18"' "a patch bump does not move the series"

echo "major release 0.18.1 -> 1.0.0"
"$here/bump-screen-engine.sh" "1.0.0" "$tmp" >/dev/null
check builtin/default 'byonk: "1.0"' "major release moves to 1.0"

echo "only a x.y.z version is accepted"
# "0.18" is the dangerous one: chopping the last dot-component off it yields the
# plausible-looking series "0", which no emptiness check catches.
for bad in "5" "0.18" "1.2.3.4" "x.y.z" ""; do
  if "$here/bump-screen-engine.sh" "$bad" "$tmp" >/dev/null 2>&1; then
    echo "  FAIL: accepted '$bad'"; fail=1
  else
    echo "  ok: rejected '$bad'"
  fi
done

if [ "$fail" -eq 0 ]; then
  echo "OK: bump-screen-engine tests passed"
else
  echo "FAIL: bump-screen-engine tests failed"; exit 1
fi
