Paste this into Claude Code as your first message (after running `claude` inside this folder):

---

Read CLAUDE.md, docs/DESIGN.md and docs/ARCHITECTURE.md fully, and open design/koma-concept-12.html plus the screenshots in design/screens to understand the look.

Then start **Phase 1 (Scaffold)** only:
1. Check that Node, Rust (MSVC) and the Visual Studio C++ build tools are installed; tell me exactly what's missing and how to install it before continuing.
2. Create a Tauri 2 + Angular app in this folder, following the folder layout in ARCHITECTURE.md.
3. Frameless window with the custom ink title bar (minimize, maximize, close).
4. Add `src/styles/tokens.css` with the light and dark tokens from DESIGN.md, bundle the four fonts locally, and add a light/dark toggle that follows the Windows theme by default.
5. Build the shared nav bar and a placeholder Home with the "№07" title block, so I can check that fonts and colors match the concept.

When it runs with `npm run tauri dev`, stop and tell me what to check. Don't start Phase 2 until I say so.
