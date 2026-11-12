# KOMA architecture

## Why Tauri 2 + Angular + Rust
- The UI is web tech, and the developer already knows **Angular**. The concept designs are HTML/CSS, so they port directly.
- **Rust** gives fast, memory-safe access to huge ZIP files and multi-threaded image decoding.
- Typical Tauri app: ~3–10 MB installer and ~40–80 MB idle RAM, versus ~120–200 MB and ~150–400 MB for Electron.
- Windows uses **WebView2**, which ships with Windows 10/11, for rendering.

### Prerequisites (Windows)
- Node.js LTS
- Rust (`rustup`, MSVC toolchain)
- Visual Studio Build Tools with the "Desktop development with C++" workload
- WebView2 runtime (preinstalled on Windows 11)

## Folder layout
```
koma/
  CLAUDE.md
  docs/                 DESIGN.md, ARCHITECTURE.md
  design/               concept HTML + screenshots (reference only, not shipped)
  src/                  Angular app
    app/
      core/             theme service, tauri api wrappers, keyboard shortcuts
      shared/           title-bar, nav, cover, seg-progress, button, chip, table, stamp
      features/
        home/           carousel, info column, reading card, up next, catalog
        library/
        series/
        reader/         spread, single, strip views; thumbnail strip; scrubber
        import/
        settings/
    styles/             tokens.css (light/dark), fonts.css, textures.css, motion.css
    assets/fonts/       Anton, Archivo, IBM Plex Mono, Noto Sans JP (woff2)
  src-tauri/
    src/
      main.rs
      archive/          CBZ backend (zip index, entry reader, natural sort), PDF backend (PDFium), image sniffing
    pdfium/             pdfium.dll (bblanchon/pdfium-binaries, build 8125) and its licences; shipped with the installer
      library/          series/volume model, json store, file-name parser, ComicInfo.xml
      import/           import jobs, progress events
      cache/            page LRU cache, preload scheduler, thumbnail disk cache
      protocol.rs       koma:// handler
      commands.rs       tauri commands (thin layer)
```

## Large-CBZ strategy (the core requirement)
1. **Open = read the index only.** Seek to the end of the file, find the End of Central Directory record (plus ZIP64 records for files over 4 GB or with over 65,535 entries), and read the central directory. That gives every page's name, offset and size in milliseconds, whatever the file size. Target: open a 5 GB file in under 300 ms.
2. **Pages are read on demand.** Most CBZs store images uncompressed ("stored"), so a page is a single seek plus read. Deflated entries are streamed through the decoder.
3. **Order pages** by natural sort on the file name (`page2` before `page10`). Skip non-images and `__MACOSX` entries.
4. **Serve pages through a custom protocol**: `koma://page/{volumeId}/{index}?w={targetWidth}`, registered with `register_asynchronous_uri_scheme_protocol`. The `<img>` tag loads it directly, with no base64 and no JSON.
   - If `w` is given, Rust decodes and resizes to the exact on-screen pixel width (Lanczos3) so pages stay sharp and small in memory; otherwise it returns the original bytes.
5. **Caching**
   - RAM: an LRU of decoded or resized pages, 1 GB by default (configurable).
   - Preload +6 / −2 pages around the current one on a background thread pool (rayon); the thumbnail strip shows which pages are cached.
   - Disk: covers and thumbnails as WebP in `%LOCALAPPDATA%\Koma\thumbs`, keyed by a hash of path + size + modified time.
6. **Long strip (manhwa)**: read each image's width and height from its header only (no decode), so the virtual scroll layout is known up front. Render only the visible slices plus a buffer. The next chapter's index is opened before the current one ends.
7. **CBR (RAR)**: solid RAR archives can't be read page by page. Offer a one-click "Convert to CBZ" instead of supporting them natively in v1.

## PDF files
Some manhwa only exist as PDF: one chapter per file, or many chapters merged into one. A PDF is a volume like a CBZ, behind the same `Archive` interface (`archive/pdf.rs`), so import, covers, thumbnails, the reader and the watcher treat both alike. The file's content (`%PDF-` header), not its extension, decides the backend.
1. **Open = read the index only.** PDFium loads the file through a reader and pulls only the parts it needs, so a multi-GB merged PDF is never held in memory.
2. **A page that is one JPEG scan is passed through untouched.** This is the usual manga PDF. The embedded JPEG file is served as it is: original quality, no decode. The page must be a single upright grey or RGB `DCTDecode` image that fills the page.
3. **Any other page is drawn by PDFium** (PNG-style images, several images, text, vectors) at the resolution of its sharpest image, or 200 DPI when it has none, and served as lossless PNG. Greyscale pages stay one channel. A drawn page is capped at 40 megapixels.
4. **Page sizes** for the long strip come from the page list and image metadata; nothing is drawn.
5. **Covers and thumbnails** of drawn pages are rendered small directly, not full size then shrunk.
6. **Threads.** PDFium is not thread-safe. `pdfium-render` serializes all calls, and each open document has its own mutex, so PDF work runs one page at a time (CBZ reads stay fully parallel).
7. **Not supported:** password-protected PDFs (reported in the import queue). PDFs have no `ComicInfo.xml`; the file name decides the series. Copied PDFs are filed as `Series\Vol NN.pdf`.

`pdfium.dll` is loaded from next to the app (in development, from `src-tauri/pdfium/`). To update it, replace the DLL with a newer build from bblanchon/pdfium-binaries and move the `pdfium_latest` feature of `pdfium-render` to match.

## Library model (no database; JSON files)
`%APPDATA%\Koma\library.json` (writes are atomic: write a temp file, then rename):
```json
{
  "version": 1,
  "series": [{
    "id": "paper-ronin",
    "title": "Paper Ronin",
    "titleJp": "ペーパーローニン",
    "type": "manga",
    "direction": "rtl",
    "volumes": [{
      "id": "uuid",
      "number": 7,
      "path": "D:\\Manga\\Paper Ronin\\Paper Ronin v07.cbz",
      "storage": "linked",
      "size": 1395864371,
      "mtime": 1731300000,
      "pages": 208,
      "progress": { "page": 142, "updatedAt": "2026-11-11T18:00:00Z" },
      "sessions": [{ "date": "2026-11-11", "from": 104, "to": 142 }]
    }]
  }]
}
```
- `storage` is `"linked"` (file stays where it is) or `"copied"` (copied into the KOMA library folder as `Series\Vol NN.cbz`).
- `settings.json` sits next to it: theme, accent, reduce motion, cache size, preload range, default direction.

## File-name parser
Try in order; the first match wins:
1. `ComicInfo.xml` inside the archive (`Series`, `Number`, `Manga=YesAndRightToLeft`)
2. `{Series} v{NN}` / `{Series} Vol. {N}` / `{Series} Volume {N}`
3. `{Series} {NN}-{NN}` (omnibus, covering a range of volumes)
4. `{Series} c{NNN}` / `Ch {N}` (chapter files, mainly manhwa)
5. Otherwise "unmatched": the user picks the series in the import queue.

Strip tags like `[Group]`, `(Digital)`, `(2019)` before matching. Unit-test the parser with at least 30 real-world-style names.

## Testing
- PDF tests generate small PDFs (JPEG-scan, raw-image and vector pages) with a helper in `testutil.rs`.
- A Rust test helper generates a CBZ with N stored JPEGs (e.g. 2,000 × 3 MB ≈ 6 GB, which forces ZIP64) in a temp folder.
- Benchmarks to track: open time, first-page time, page-turn time with a warm cache, and RAM after reading 500 pages.
- Targets: open in under 300 ms; first page shown in under 150 ms; cached page turn in under 16 ms; RAM stays under cache size + 150 MB.

## Allowed dependencies (ask before adding others)
- Rust: `tauri` 2, `zip` (or a small custom central-directory reader), `pdfium-render` (plus the bundled `pdfium.dll`), `image`, `fast_image_resize`, `rayon`, `lru`, `serde`, `serde_json`, `quick-xml`, `notify` (folder watching), `uuid`, `directories`, `natord`
- Angular: Angular CDK (virtual scroll), `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs`
