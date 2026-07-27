# What a HiFi bank costs, measured on the committed corpus

Informative. Recorded at v0.12 story #434, the first derived bank. Nothing
depends on this note; it exists so the numbers behind two decisions are on
record, and because one of them is a finding rather than a confirmation.

## What was measured

Every PNG-decodable asset in the committed corpus, packed through
`dashpack::bank::pack_bank` under HiFi and under LoFi. Canonical size is the
committed file on disk; resident size is the complete KTX2 file the packer
produced, before container alignment. Release build, arm64.

The GIF and JPEG corpus fixtures are absent because the packer takes decoded
texels and the canonical-to-texels ingest is a later story; only PNG is decoded
in tests today.

### HiFi

| asset               | extent  | kind           | rung         |   canonical |    resident |     ratio |
| ------------------- | ------- | -------------- | ------------ | ----------: | ----------: | --------: |
| `v03-paint`         | 16x16   | image          | astc-8x8     |          93 |         249 |     2.677 |
| `import-image-fill` | 380x380 | image          | astc-6x6     |     171 556 |      21 026 |     0.123 |
| `atlas/inter-ascii` | 512x256 | distance field | uncompressed |      63 940 |      73 703 |     1.153 |
| `atlas/arabic`      | 512x256 | distance field | uncompressed |      96 675 |      98 538 |     1.019 |
| **total**           |         |                |              | **332 264** | **193 516** | **0.582** |

### LoFi

| asset               | rung         |   canonical |    resident |     ratio |
| ------------------- | ------------ | ----------: | ----------: | --------: |
| `v03-paint`         | astc-8x8     |          93 |         249 |     2.677 |
| `import-image-fill` | astc-12x12   |     171 556 |       7 961 |     0.046 |
| `atlas/inter-ascii` | uncompressed |      63 940 |      73 703 |     1.153 |
| `atlas/arabic`      | uncompressed |      96 675 |      98 538 |     1.019 |
| **total**           |              | **332 264** | **180 451** | **0.543** |

## Three things the numbers say

**The saving is real and it is concentrated in the large image.** HiFi takes
`import-image-fill` to 12.3 % of its canonical size, LoFi to 4.6 %. This is what
the profiles are for, and the escalation earns its keep: HiFi rejected three
rungs before 6x6 held at 0.2133 % differing, while LoFi's looser band accepted
the cheapest rung on the ladder at 0.0000 %. The bands are what chose, not
arithmetic that would have chosen the same rung either way.

**A small asset costs more under a profile than under RAW.** `v03-paint`'s image
is 16x16 — one ASTC block at every footprint the ladder offers — so its resident
payload is a block plus KTX2 framing, against a 93-byte PNG that carries
neither. The ratio is 2.677 and it is a property of the format, not a defect. It
is asserted in `goldens/tooling/tests/derived_bank.rs` so that a change making
it accidentally smaller is explained rather than welcomed.

**The lossless path currently makes distance fields bigger, and that is a
finding.** An MSDF atlas may never be encoded lossily
(`docs/decisions/baked-vector-msdf-field.md`), which is right and is measured:
even at 4x4, the finest footprint, the committed atlases put 8.6 % and 8.9 % of
their texels beyond a delta of 8. But the _lossless_ representation the ladder
terminates at is uncompressed 8-bit RGBA wrapped in KTX2 and zstd-compressed,
and on this content that lands 15.3 % and 1.9 % **above** the canonical PNG,
whose own filtering plus deflate does better.

So for the distance-field class today, HiFi ships more bytes than RAW would.
That is not what a production profile is for. The fix is a contract question and
not an assembly one — the natural shape is for a lossless-only class to fall
back to `Binding::Canonical` when the derivation is not smaller, which the bank
already represents and the manifest already handles, since an identity binding
simply writes no row. It is filed rather than fixed here: changing which rung a
profile binds is the band story's territory, and story #434 is the one story in
the slice permitted to move a golden, so it deliberately did not also change
what the packer chooses.

## Multi-bank files

The asset-pipeline plan's position, unchanged by these numbers: multiple banks
in one file are possible and load-time-neutral under mmap, but each one costs
flash and OTA size in full. The default stays one bank per target, with
multi-bank reserved for development and demonstration toggles. Nothing measured
here argues otherwise — the totals above are per bank, so a second bank in the
same file would add its own 180 to 194 KB.
