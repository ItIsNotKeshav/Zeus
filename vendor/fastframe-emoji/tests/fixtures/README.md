# Test fixtures

`NotoColorEmoji-subset.ttf` is Noto Color Emoji 2.051 (the copy ZapFast
bundles), cut down to the characters and sequences the tests draw:

```sh
pyftsubset NotoColorEmoji.ttf --text="😀👍🏽👨‍👩‍👧🇩🇪#️⃣❤️" \
  --layout-features='*' --output-file=NotoColorEmoji-subset.ttf
```

It keeps the CBDT strike and the GSUB ligatures, so shaping and drawing are
tested without the machine's fonts. SIL Open Font License 1.1: see
[OFL.txt](OFL.txt).
