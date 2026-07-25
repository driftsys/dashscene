# Inter (Latin/Greek/Cyrillic)

    source   github.com/rsms/inter
    release  Inter-3.19.zip (v3.19, 2021-06-18)
             sha256 150ab6230d1762a57bebf35dfc04d606ff91598a31d785f7f100356ecdcc0032
    build    "Inter Desktop" — unhinted static CFF (the runtime never uses
             TT hints, and this release ships no unhinted TrueType static)
    license  OFL 1.1 — see OFL.txt (one licence covers every face here)

The family real Figma files are authored in, and the family the pinned cascade
resolves a document's `TextStyle::family` against (story #385,
`docs/decisions/corpus-ships-inter.md`). Do not modify a file; replace it
wholesale (and update this README) when a version bump is deliberate.

## Faces

Four static upright faces, all from the same release archive, path
`Inter Desktop/`:

| file                 | CSS weight | bytes   | sha256 (first 16)  |
| -------------------- | ---------- | ------- | ------------------ |
| `Inter-Regular.otf`  | 400        | 258 992 | `a7e791e8f5a0fb02` |
| `Inter-Medium.otf`   | 500        | 269 692 | `99dab2bdcb613c4c` |
| `Inter-SemiBold.otf` | 600        | 270 760 | `8c1990b6012254ea` |
| `Inter-Bold.otf`     | 700        | 271 436 | `1e9dfd6a6e33ac63` |

All four report `Version 3.019;git-0a5106e0b` and unitsPerEm 2816, and their
`usWeightClass` values are 400/500/600/700 respectively. The intermediate and
extreme weights the archive also ships (Thin, ExtraLight, Light, ExtraBold,
Black) and every italic are deliberately not committed, for the reason
`corpus/fonts/noto-sans/README.md` gives: a weight with no committed atlas has
nothing to render with, and the CSS Fonts 4 matching rule
(`crates/dashscene-typeset/src/text/weight.rs`) resolves a request for one of
them to a committed neighbour, reporting `text.weight-substituted`.

Weight 500 is committed here although #368 deliberately excluded Noto Sans
Medium. The reason differs: no committed fixture requests Noto Sans at 500,
whereas the Landify hero — the live fidelity target — carries Inter Medium
nodes.

## Why v3.19 and not v4.x

Figma's bundled Inter is a 3.x build, and the committed fixtures prove it
without needing access to Figma's binaries. Every Inter TEXT node Figma
captured records an automatic line height that is exactly `3408/2816` of the
font size, to f32 precision:

| node                                     | size | Figma's `lineHeightPx` |
| ---------------------------------------- | ---- | ---------------------- |
| `liga-text` `1:5`, `effects-2025` `1:7`  | 12   | 14.522727012634277     |
| `grid-basic` `1:16`, and five others     | 14   | 16.94318199157715      |
| `lowering-baseline` `1:4`                | 24   | 29.045454025268555     |
| `lowering-baseline` `1:5`                | 40   | 48.409088134765625     |

That ratio identifies the metrics of the face Figma measured with:

| release    | unitsPerEm | hhea asc/desc/gap | ratio        |
| ---------- | ---------- | ----------------- | ------------ |
| Inter 3.19 | 2816       | 2728 / -680 / 0   | 1.2102272727 |
| Inter 4.1  | 2048       | 1984 / -494 / 0   | 1.2099609375 |

Only the 3.x metrics produce Figma's recorded values. v3.19 is the last 3.x
release, so it is the pin.

Two honest limits on that evidence. The fingerprint is a **vertical-metrics**
match, so it fixes the major version but does not by itself separate 3.x point
releases, whose metrics are identical. And it says nothing about letterform or
spacing agreement — that is what the render diff against Figma's own
`GET /images` export measures, not this table.

## Why the family name is declared rather than read from the face

`Inter-Medium.otf` and `Inter-SemiBold.otf` declare name ID 1 as
`Inter Medium` and `Inter Semi Bold`, not `Inter` — the four-styles-per-family
convention that predates the typographic family name in name ID 16. The
cascade therefore declares a family's name once, in code, and never derives it
per face; a per-face reading of name ID 1 would put the 500 and 600 weights in
families of their own and stop a document asking for `Inter` from reaching
them.
