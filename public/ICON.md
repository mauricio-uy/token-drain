# Tok-Ching icon

`tok-ching.svg` is the original app mark: a cash register, a token with a
negative-space T, and three ringing strokes. All visible pixels are black;
cutouts and the background are transparent. The mark is covered by the
repository's MIT license.

Generate native assets with the installed Tauri CLI:

```powershell
npm run tauri icon -- public/tok-ching.svg --output <temporary-output-directory>
```

Copy the generated desktop files matching `src-tauri/icons/` into that
directory. Keep the SVG as the editable source and favicon. Mobile outputs are
not used by this Windows application. Preview at 16, 24 and 32 pixels before
shipping changes; do not add gradients, outlines or a colored backdrop.

Flat black intentionally has low contrast on dark surfaces; it follows the
app's monochrome identity rather than changing color with the system theme.
