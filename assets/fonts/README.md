# Document Fonts

These unmodified variable fonts are embedded by `markdown_to_pdf`, so generation
does not depend on fonts installed on the host. `NotoSansCJK.otf` is embedded
only when IronFlow is built with the `cjk-fonts` Cargo feature (the `-full`
release binaries and the container image); other builds can load it, or any
other font, from the directory named by `IRONFLOW_FONT_DIR`. DOCX references
font families and leaves font substitution and pagination to the document
viewer.

Source: [Google Fonts](https://github.com/google/fonts/tree/e7db6ca603fd4862010383ff17ffd6cc4ffac1c0/ofl),
revision `e7db6ca603fd4862010383ff17ffd6cc4ffac1c0`.
The CJK source is [Noto CJK](https://github.com/notofonts/noto-cjk/tree/f8d157532fbfaeda587e826d4cd5b21a49186f7c),
revision `f8d157532fbfaeda587e826d4cd5b21a49186f7c`.
All six fonts use the [SIL Open Font License](OFL.txt).

| File | Upstream Path | SHA-256 |
|------|---------------|---------|
| NotoSerif.ttf | ofl/notoserif/NotoSerif[wdth,wght].ttf | 4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713 |
| NotoSerif-Italic.ttf | ofl/notoserif/NotoSerif-Italic[wdth,wght].ttf | e87acbc6c0efd0d9a20d6a8cbbda2b266c14be3a3a6f5af8ec9d7b2460570ad1 |
| NotoSansMono.ttf | ofl/notosansmono/NotoSansMono[wdth,wght].ttf | 2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081 |
| NotoSansArabic.ttf | ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf | 63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e |
| NotoSansHebrew.ttf | ofl/notosanshebrew/NotoSansHebrew[wdth,wght].ttf | 7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88 |
| NotoSansCJK.otf | Sans/Variable/OTF/NotoSansCJKsc-VF.otf (Noto CJK) | 2745e9681cb9d8a5c8901b62c9e1bd98c9c774365fc3b84dd467621013b51cd3 |

PDF text supports the bundled fonts' Latin, Greek, Cyrillic, Arabic, Hebrew and
pan-CJK coverage, with bidi-aware shaping and automatic fallback. Other scripts
and emoji are not universally covered; missing glyphs fail explicitly.
Generation does not claim PDF/A or PDF/UA conformance.

These trusted fonts are registered once per process without system font discovery.
The Arabic and Hebrew fonts add about 1 MiB and the pan-CJK font about 29 MiB of
uncompressed font data to a binary that includes them. PDF output embeds
subsets, not the full CJK font.

DOCX files reference the Noto family names but do not embed these fonts. Install
them in the consuming viewer's environment or supply suitable fallback fonts.
Local visual QA used an isolated Fontconfig file pointing at this directory,
not changes to the user's global font installation.
