# Token Drain icon

[`public/token-drain.svg`](../public/token-drain.svg) is the editable Outlet
mark: a token inside an open circular reservoir and a second token escaping
below. Its geometry uses `currentColor`, with a transparent background. The
generated desktop assets are black on transparency. The mark is covered by
the repository's MIT license.

[`docs/social-preview.png`](social-preview.png) is the 1280 by 640 GitHub social
preview image. Upload it in the repository settings under **Settings → General →
Social preview** after changing it in the repo.

Generate native assets with the installed Tauri CLI:

```powershell
npm run tauri icon -- public/token-drain.svg --output <temporary-output-directory>
```

Copy the generated desktop files matching `src-tauri/icons/` into that
directory. Keep the SVG as the editable source and favicon. Mobile outputs are
not used by this Windows application. Preview at 16, 24 and 32 pixels before
shipping changes. Preserve the circular stroke, spacing and transparent backdrop.

Flat black intentionally has low contrast on dark surfaces; it follows the
app's monochrome identity rather than changing color with the system theme.
