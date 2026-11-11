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
      archive/          zip index, entry reader, natural sort, image sniffing
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
- A Rust test helper generates a CBZ with N stored JPEGs (e.g. 2,000 × 3 MB ≈ 6 GB, which forces ZIP64) in a temp folder.
- Benchmarks to track: open time, first-page time, page-turn time with a warm cache, and RAM after reading 500 pages.
- Targets: open in under 300 ms; first page shown in under 150 ms; cached page turn in under 16 ms; RAM stays under cache size + 150 MB.

## Allowed dependencies (ask before adding others)
- Rust: `tauri` 2, `zip` (or a small custom central-directory reader), `image`, `fast_image_resize`, `rayon`, `lru`, `serde`, `serde_json`, `quick-xml`, `notify` (folder watching), `uuid`, `directories`, `natord`
- Angular: Angular CDK (virtual scroll), `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs`
