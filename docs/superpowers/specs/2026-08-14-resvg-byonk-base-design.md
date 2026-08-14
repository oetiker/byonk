# Building byonk-base in the resvg fork

**Date:** 2026-08-14
**Repo:** `oetiker/resvg` (fork of `linebender/resvg`)
**Deliverable:** a `byonk-base` branch on current upstream, carrying every text
feature byonk needs

Companion spec: `2026-08-14-byonk-resvg-integration-design.md`, which consumes
this branch. **This spec comes first.** byonk integrates once, when `byonk-base`
is ready.

## Status — 2026-08-14

**Built and green.** `byonk-base` exists at `b67da7c0`, 18 commits ahead of
`upstream/main`, passing 1808 tests (1746 reference renders) with no new clippy
warnings.

| Step | State |
|---|---|
| Merge #1115 and #1116 into `byonk-base` | done (`b877fa04`, `97a95ca0`) — three import-level conflicts, resolved |
| `feat/font-hinting-resolver` | done (`a675a998`), merged at `01f89ef3` |
| Hinting × bitmap-strike combination tests | done (`ce9ca399`) |
| `FontResolver::select_bitmap` | done (`b67da7c0`) |
| Hand off to the integration spec | ready |

Remaining: nothing blocking. The branch is consumable.

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
| Per-font bitmap strike choice | **New** — `FontResolver::select_bitmap`, on `byonk-base`. |
| `FaceInfo::bitmap_strikes` | Moves out of resvg entirely; see the integration spec. |
| Glyph outline LRU cache | Dropped permanently. |
| PNG DPI | PR #1118, closed upstream. Dropped permanently. |
| Hinting via custom SVG attributes | Rejected upstream. Superseded by the resolver. `feat/hinting-css-properties` is **kept** for now rather than deleted, though nothing depends on it. |

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

## Choosing whether a font's strikes are used

Hinting has nothing to offer a bitmap glyph: a strike arrives as an image, so
there is no outline to grid-fit. That leaves *which of the two to draw* as the
only lever, and #1115 left it fixed — a strike was used whenever the size
matched, and every other size fell back to the outline.

`FontResolver::select_bitmap` makes it a choice, per font, beside the hinting
selector:

```rust
pub type BitmapSelectionFn<'a> = Box<dyn Fn(ID, f32, &Database) -> bool + Send + Sync + 'a>;
```

The resolved face, the size in user units, and the database; `false` sends the
glyph to the outline. Declining a font's strikes is therefore also what makes it
hintable, which is why the two selectors belong together.

The default allows strikes for every font, so an unset resolver behaves exactly
as before. #1115's exact-size rule is unchanged: this gates strike use, it does
not add scaling of a strike to a size it was not drawn for.

## Testing

- The resolver branch renders byte-identically to #1116 when no resolver is set.
  This is the compatibility guarantee and the primary test.
- A mixed-font reference test: one document, a bitmap font and an outline font,
  different hinting for each — the case that justifies the feature.
- Existing #1115 and #1116 tests keep passing; neither branch is modified.
- `byonk-base` itself builds and passes the full suite after each merge. The tip
  is what byonk consumes, so a green tip is the deliverable, not a nicety.

Two lessons from doing it, worth keeping for the next round:

**A reference-backed test can pass vacuously.** Both combination tests compare
against references that must differ for the comparison to mean anything, so the
difference was measured: 476 pixels between the unhinted and hinted renders of
`mixed-fonts.svg`. Without that check, a fixture insensitive to hinting would
have produced two green tests proving nothing.

**Generating a reference from your own new code proves nothing on its own.**
`monochrome-no-strikes.png` could only come from the implementation it tests, so
the wiring was reverted with the test in place: it then failed by 690 pixels
while the control — declining a family the file never uses — kept passing. That
is the evidence the test catches the feature's absence without the selector
over-applying.

The combination tests live on `byonk-base` rather than on either feature branch,
because the combination only exists where both are merged. They are therefore in
neither PR, and are a follow-up contribution if upstream takes both.

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

1. ~~Create `byonk-base` from `upstream/main`; merge `feat/bitmap-mask-glyphs`
   and `feat/font-hinting`. Build and test the tip.~~ done
2. ~~Build `feat/font-hinting-resolver` on `feat/font-hinting`. Verify the
   no-resolver-set equivalence against #1116.~~ done
3. ~~Re-merge the tip; build and test.~~ done
4. ~~Cover the hinting × bitmap-strike combination.~~ done
5. ~~Add `FontResolver::select_bitmap`.~~ done
6. Hand off to the integration spec.

`feat/hinting-css-properties` is kept rather than deleted. It carries the
rejected custom-attribute design and nothing depends on it, but it costs nothing
to leave in place.

## Maintaining this branch

The merge is not conflict-free, so budget for it after an upstream bump. Merging
#1116 into #1115 produced three conflicts, all import-level, all from the two
branches reworking the same text code:

- `converter.rs` — #1115 relocated `BitmapImage` into a new `text::bitmap`
  module, so #1116's `crate::flatten::` import path is stale.
- `flatten.rs` — `BitmapData`/`BitmapFormat` moved to `bitmap.rs`, leaving
  flatten needing only `HintingInstance`.
- `CHANGELOG.md` — both add an Unreleased entry; keep both.

Worth noting for the next person: `git merge-tree` and GitHub both reported the
two branches as cleanly mergeable, because each was tested **against
`upstream/main` independently**. That says nothing about whether they merge with
*each other*. Check the pairwise merge, not just each branch's PR status.
