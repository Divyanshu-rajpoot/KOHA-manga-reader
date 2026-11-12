# KOMA — offline manga & manhwa reader for Windows

Read this file first, every session. Then read `docs/DESIGN.md` and `docs/ARCHITECTURE.md` before writing UI or backend code.

## What we are building
A fast, offline desktop reader for **locally imported CBZ and PDF files** (manga and manhwa).
- The user imports CBZ or PDF files (or a folder of them). A PDF can be one chapter or many chapters merged. **Each file is a volume**; volumes are grouped into **series**.
- No accounts, no internet, no online sources, no telemetry. Everything stays on the user's PC.
- Must handle **very large CBZ files (1–10+ GB, ZIP64)** without extracting and without freezing the UI.
- Windows first (Windows 10/11, x64). Other platforms later.

## Design
- The chosen design is **Concept 12, "Ink Archive"**, with a **light ("paper") and dark ("ink") theme**.
- Visual reference: `design/koma-concept-12.html`. Open it in a browser. In its picker, **Concept 12 · Ink Archive** is the chosen design; Concepts 2, 9 and 4 are there only as history.
- Screenshots: `design/screens/` (`light-*.png`, `dark-*.png`).
- All tokens, components and motion rules: `docs/DESIGN.md`. **Never hard-code colors; use the CSS variables.**
- The book covers in the concept are generated placeholders. The real app shows the first page of each CBZ or PDF as its cover.

## Stack (decided)
- **Tauri 2** desktop shell
- **Angular** (latest stable, standalone components, signals) for the UI, in `src/`
- **Rust** for everything that touches files, in `src-tauri/`
- Details and reasons: `docs/ARCHITECTURE.md`

## Rules for working on this repo
1. Work in the phases listed below, one at a time. Finish a phase, run it, show me, then stop and wait for approval before the next phase.
2. Never load a whole CBZ or PDF into memory. Read the ZIP central directory, then read single entries on demand. PDFs are read on demand through PDFium (see ARCHITECTURE → PDF files).
3. Page images go to the UI through the custom `koma://` protocol, never as base64 or JSON over IPC.
4. All file I/O, decoding and thumbnailing runs in Rust on background threads. The UI thread must never block.
5. Keep components small. Each screen is its own Angular feature folder.
6. Animations follow `docs/DESIGN.md` → Motion, and must respect "Reduce motion" (the OS setting plus the in-app toggle).
7. Write unit tests for the Rust archive, file-name parser and library code. Test with a generated multi-GB CBZ (see ARCHITECTURE → Testing).
8. Ask me before adding any new dependency that isn't listed in ARCHITECTURE.md.

## Phases
1. **Scaffold**: Tauri 2 + Angular app runs; frameless window with custom title bar; theme tokens loaded; light/dark toggle works.
2. **Archive engine (Rust)**: open CBZ/ZIP/ZIP64, list image entries in natural sort order, read one page by index, serve it via `koma://`. Benchmark: open a 5 GB CBZ in under 300 ms.
3. **Import + library**: import files or folders, file-name parser (`{Series} v{NN}.cbz`, `{Series} Vol {N}.cbz`, `{Series} {NN}-{NN}.cbz`, ComicInfo.xml override), choose "keep in place" or "copy to library folder", library saved as JSON in `%APPDATA%\Koma`, covers and thumbnails cached in `%LOCALAPPDATA%\Koma\thumbs`.
4. **Reader**: spread (RTL), single and long-strip modes; page cache plus preloading; thumbnail strip; keyboard shortcuts; remembers progress.
5. **Home**: the 3D cover carousel driving the left info column, reading card, up next, catalog table.
6. **Library and series screens**: cover and details views, filters, sort, volume grid with progress.
7. **Settings and polish**: settings screen, reduce motion, file watcher, packaging (MSI/NSIS installer).

## Commands (fill in once scaffolded)
- Dev: `npm run tauri dev`
- Build: `npm run tauri build`
- Rust tests: `cd src-tauri && cargo test`
- 6 GB archive benchmark (writes a temp file, then deletes it): `cd src-tauri && cargo test benchmark_6gb -- --ignored --nocapture`
