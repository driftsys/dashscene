# Noto Sans Arabic

    source   github.com/notofonts/arabic
    release  NotoSansArabic-v2.013
    build    unhinted/ttf (the runtime never uses TT hints)
    license  OFL 1.1 — see OFL.txt

Test and golden fixture font for the Arabic text stack (#33, #34, #35).
It carries GSUB, GPOS, and cmap tables, so it exercises Arabic
contextual-form and ligature substitution — the coverage the GSUB
charset closure (#34) computes. Do not modify the file; replace it
wholesale (and update this README) when a version bump is deliberate.
