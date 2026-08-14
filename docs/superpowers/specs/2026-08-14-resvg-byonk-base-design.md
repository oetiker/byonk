# Building byonk-base in the resvg fork

**Date:** 2026-08-14
**Repo:** `oetiker/resvg` (fork of `linebender/resvg`)
**Deliverable:** a `byonk-base` branch on current upstream, carrying every text
feature byonk needs

Companion spec: `2026-08-14-byonk-resvg-integration-design.md`, which consumes
this branch. **This spec comes first.** byonk integrates once, when `byonk-base`
is ready.

## Problem

byonk pins `resvg`, `usvg`, and `fontdb` to the fork's `skrifa` branch
(byonk `Cargo.toml:104-106`) — PR #1004, the large port of resvg's text stack
from ttf-parser/rustybuzz to harfrust/skrifa. It was closed upstream and is cut
from `b8e58f5a` (v0.46.0), so byonk has been frozen at v0.46 while upstream
shipped v0.47, v0.48, and v0.48.1.

Upstream has since adopted the port itself (`4d76f92a`, released in v0.48.0).
`upstream/main` (`1398abb2`) depends on `harfrust 0.12`, `skrifa 0.44`, and
`fontdb 0.24`. The reason `skrifa` existed is gone.

What remains is a small set of deltas that upstream has not taken yet. This spec
assembles them on current upstream as `byonk-base`.

## Relationship to upstream

**`byonk-base` proceeds regardless of whether upstream merges the PRs.** Merging
is desirable but not a gate. If #1115 and #1116 land, their commits become part
of `upstream/main` and the corresponding merges into `byonk-base` become no-ops —
the branch shrinks on its own. Nothing about byonk's integration changes either
way.

## What the fork still carries

| Feature | Status |
|---|---|
| harfrust/skrifa port | Upstream. Nothing to carry. |
| Variable fonts | Upstream (#997, merged). |
| Default `wght` 400 for variable fonts | Upstream (#1099). `parser/text.rs:411` always pushes `wght`; `resolve_font_weight` defaults to 400. |
| Bitmap glyph rendering (mono/grayscale/BGRA) | **PR #1115, open** — carried. |
| Font hinting | **PR #1116, open** — carried. |
| Per-font hinting | **New** — `feat/font-hinting-resolver`. |
| `FaceInfo::bitmap_strikes` | Moves out of resvg entirely; see the integration spec. |
| Glyph outline LRU cache | Dropped permanently. |
| PNG DPI | PR #1118, closed upstream. Dropped permanently. |
| Hinting via custom SVG attributes | Rejected upstream. Superseded by the resolver. `feat/hinting-css-properties` is deleted. |

Recorded, not addressed: upstream always pushes the `wght` variation coordinate
while `wdth`, `ital`, and `slnt` are pushed only when non-default
(`parser/text.rs:429,435,441`), so a font whose default instance is `wdth 75`
stays condensed for markup saying `font-stretch: normal`. A genuine upstream bug
and a plausible future PR; out of scope here.

## Branch topology

```
upstream/main (1398abb2)
├── feat/bitmap-mask-glyphs          #1115 — untouched
├── feat/font-hinting                #1116 — untouched
│   └── feat/font-hinting-resolver   new — depends on #1116
└── byonk-base                       tip = merge(bitmap, resolver)
```

#1115 and #1116 are independent features, so they stay independent branches and
each remains separately submittable. Only the resolver stacks, because it
genuinely depends on the hinting API.

**The two open PR branches are never force-pushed.** Both are already
`MERGEABLE`/`CLEAN` against current upstream, so the tip is built by merging, not
rebasing, and the reviewed PRs stay undisturbed. Rebase a feature branch only
when upstream actually conflicts with it.

Regenerating the tip after an upstream bump:

```
git checkout byonk-base && git reset --hard upstream/main
git merge feat/bitmap-mask-glyphs
git merge feat/font-hinting-resolver
```

Automate only if that becomes tedious.

## The hinting resolver

### Background

PR #1116 originally let a document control hinting through custom SVG attributes.
RazrFalcon rejected that outright: *"strongly against `ResvgHinting` attribute.
We should not pollute the SVG tree"*, and *"resvg strongly follows the spec and
doesn't add anything extra. [...] Nothing custom."*

Two things survived. `text-rendering="geometricPrecision"` stayed as per-element
control, because it is a spec property. And expressing per-font hinting through a
resolver was accepted in advance: *"As for tweaking hinting via `FontResolver` —
sure. Maybe someone would find it useful."*

The capability was never rejected. Only the mechanism was.

### Why a resolver is the right shape

Hinting is applied per glyph, after font fallback has been resolved. A document
that falls back from a variable font to a bitmap font mid-run needs different
treatment for each, which no attribute on the element can express. A font-keyed
resolver gets it right, adds nothing to the SVG tree, and keeps the markup
standard — the SVG already says which font each element uses.

This is byonk's actual case: screens mix bitmap pixel fonts such as X11Helv with
outline and variable fonts, and those want opposite treatment.

### API

RazrFalcon's wording points at `FontResolver` itself rather than a new sibling
type, so this adds a third field to `FontResolver`
(`crates/usvg/src/text/mod.rs:74`), beside `select_font` and `select_fallback`:

```rust
pub type HintingSelectionFn<'a> = Box<
    dyn Fn(ID, f32, Option<FontHintingOptions>, &Database)
        -> Option<FontHintingOptions> + Send + Sync + 'a,
>;
```

Arguments: the resolved face ID, the font size in user units, the global
`Options::font_hinting`, and the database. Returning `None` means unhinted.

The default implementation returns the passed-in global unchanged. **With no
resolver set, behaviour is identical to #1116.** That equivalence is the branch's
main test obligation.

`text-rendering="geometricPrecision"` short-circuits to unhinted *without*
consulting the resolver. A spec property outranks a host hook.

For reference, the configuration being resolved (`crates/usvg/src/text/hinting.rs`
on `feat/font-hinting`):

```rust
Options::font_hinting: Option<FontHintingOptions>   // None = off
FontHintingOptions { engine, target }
  engine: Interpreter | Auto | AutoFallback(default)
  target: Mono
        | Smooth { mode, symmetric_rendering: bool, preserve_linear_metrics: bool }
    mode: Normal(default) | Light | Lcd | VerticalLcd
```

### Integration points

The glyph outline cache needs **no change**. Its key already discriminates by
face (`parser/converter.rs:14-19`):

```rust
type OutlineCacheKey = (
    ID,                                   // face — already present
    GlyphId,
    Vec<FontVariation>,
    Option<(FontHintingOptions, u32)>,    // what GlyphHinting::cache_key() returns
);
```

`GlyphHinting::cache_key()` (`flatten.rs:36`) is only the hinting fragment of a
four-part key, and `fontdb_outline` (`converter.rs:222-236`) already keys on `ID`
and `GlyphId`. Different faces and different hinting options already occupy
different slots, so the resolver is a smaller change than it first appears.

Genuine touch points: `parser/text.rs:146` (where `state.opt.font_hinting` is
read today) and `converter.rs:232` (where the hinting fragment is assembled).

## Testing

- The resolver branch renders byte-identically to #1116 when no resolver is set.
  This is the compatibility guarantee and the primary test.
- A mixed-font reference test: one document, a bitmap font and an outline font,
  different hinting for each — the case that justifies the feature.
- Existing #1115 and #1116 tests keep passing; neither branch is modified.
- `byonk-base` itself builds and passes the full suite after each merge. The tip
  is what byonk consumes, so a green tip is the deliverable, not a nicety.

## Risks

**Upstream may still require API changes.** #1115 and #1116 await a maintainer
who has already required one rename (`hinting` -> `font_hinting` across the whole
surface). The resolver sits on #1116's types, and byonk's Lua mapping sits on the
resolver. This does not block `byonk-base`, but churn propagates downstream.

**Nothing here has been compiled or tested.** Mergeability of #1115 and #1116 was
verified textually — `git merge-tree` and GitHub agree there are no conflicting
hunks, which is not the same as the merged result building or its reference
renders matching.

## Sequence

1. Create `byonk-base` from `upstream/main`; merge `feat/bitmap-mask-glyphs` and
   `feat/font-hinting`. Build and test the tip.
2. Build `feat/font-hinting-resolver` on `feat/font-hinting`. Verify the
   no-resolver-set equivalence against #1116.
3. Re-merge the tip; build and test.
4. Delete `feat/hinting-css-properties`.
5. Hand off to the integration spec.
