# Noto Sans (Latin/Greek/Cyrillic)

    source   github.com/notofonts/latin-greek-cyrillic
    release  NotoSans-v2.015
    build    unhinted/ttf (the runtime never uses TT hints)
    license  OFL 1.1 — see OFL.txt (one licence covers every face here)

Test and golden fixture fonts for the text stack (#27, #28, #29, #30).
Do not modify a file; replace it wholesale (and update this README)
when a version bump is deliberate.

## Faces

Three static upright faces, all from the same release archive
(`NotoSans-v2.015.zip`, path `NotoSans/unhinted/ttf/`):

| file                   | CSS weight | archive size | added by     |
| ---------------------- | ---------- | ------------ | ------------ |
| `NotoSans-Regular.ttf` | 400        | 431 364      | #27          |
| `NotoSans-SemiBold.ttf`| 600        | 431 500      | #368         |
| `NotoSans-Bold.ttf`    | 700        | 432 376      | #368         |

Provenance of the #368 additions is confirmed exactly: the archive's own
`NotoSans-Regular.ttf` is SHA-256 identical to the file committed here
since #27, and its `OFL.txt` is SHA-256 identical to the committed
`OFL.txt` — so every face above comes from one release and one build
variant. The intermediate weights the archive also ships (Thin,
ExtraLight, Light, Medium, ExtraBold, Black) are deliberately not
committed: a weight with no committed atlas has nothing to render with,
and the CSS Fonts 4 matching rule — implemented in
`crates/dashscene-typeset/src/text/weight.rs` — resolves a request for
one of them to a committed neighbour, reporting the substitution as
`text.weight-substituted`.
