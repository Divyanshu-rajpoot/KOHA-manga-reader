# KOMA design system — Concept 12 "Ink Archive"

A merge of three explorations:
- **Screentone (2):** ink, halftone dots, speed lines, slanted panel edges, speech bubbles, hard red offset shadows.
- **Archive Card (9):** giant volume numbers, thick grid rules, monospace data, stamped reading cards, catalog tables.
- **Neon Seoul (4):** the rotating 3D cover carousel on Home.

Visual reference: `design/koma-concept-12.html` (open it, select Concept 12, use the Theme and Accent switches).

## 1. Color tokens
Define these as CSS variables on `:root[data-theme="light"]` and `:root[data-theme="dark"]`. Default to the Windows system theme; the user can override it in Settings.

| Token | Light "paper" | Dark "ink" | Use |
|---|---|---|---|
| `--bg` | `#F1EEE6` | `#121110` | window background |
| `--bg2` | `#E5E1D6` | `#1E1C1A` | inputs, chips, subtle fills |
| `--panel` | `#FAF8F3` | `#191816` | panels, cards |
| `--panel2` | `#FFFFFF` | `#1F1D1A` | carousel stage, bubbles, loan card |
| `--text` | `#0E0E0E` | `#EDE8DD` | body text |
| `--muted` | `#55524C` | `#A9A296` | secondary text |
| `--ink` | `#0E0E0E` | `#EDE8DD` | borders, rules, primary buttons, table headers |
| `--on-ink` | `#F1EEE6` | `#121110` | text on `--ink` |
| `--acc` | `#E3242B` | `#EC4237` | the single accent: offset shadows, progress, stamps, CTA |
| `--line` | `rgba(14,14,14,.2)` | `rgba(237,232,221,.18)` | hairlines |
| `--reader-bg` | `#E9E5DC` | `#0D0C0B` | reader stage |

- Optional accent "Archive orange": `#FF5B00` (light and dark). Offer it in Settings → Appearance.
- Rule: one accent only. Everything else is ink and paper.
- In dark mode the "Import" panel inverts (paper-colored panel with ink text). This is intentional.
- Text contrast must be at least 4.5:1 on its background.

## 2. Typography
Bundle the fonts locally (offline app; no Google Fonts CDN). All are OFL-licensed from Google Fonts.

| Role | Font | Notes |
|---|---|---|
| Display: titles, big numbers, panel headings | **Anton** 400 | always UPPERCASE; volume number "№07" at 146 px, series title 58 px, panel headings 22 px |
| Body / UI | **Archivo** 400 / 500 / 700 | 14 px base, 13 px small |
| Data: file names, sizes, page counts, tables | **IBM Plex Mono** 400 / 500 | 11–12 px, letter-spacing 0.5–1 px |
| Japanese titles | **Noto Sans JP** 500 / 700 | vertical text on covers and subtitles |

## 3. Shape and texture
- Corner radius: **0** everywhere (square).
- Borders: 2 px (controls), 3 px (panels, the Home top band, covers).
- **Offset shadow** instead of soft shadows: `box-shadow: 8px 8px 0 var(--acc)` on hover and selection; buttons use 5 px.
- Halftone dots: `radial-gradient(var(--ink) 1.1px, transparent 1.4px)` at 7 px, opacity 0.09.
- Speed-line burst: `repeating-conic-gradient(var(--ink) 0 1deg, transparent 1deg 8deg)`, opacity 0.09, rotating slowly (80 s per turn).
- Slanted panel edges on Home's mid row: `clip-path: polygon(0 0,100% 0,97% 100%,0 100%)`.

## 4. Components
- **Title bar**: custom and frameless; ink background; window buttons on the right (minimize, maximize, close, with close turning red on hover).
- **Nav**: logo (red square containing "コ" + KOMA in Anton), tabs Home / Library / Import (the active tab is an ink-filled box), search box with a 2 px border and a "Ctrl K" hint, red "+ Add CBZ" button.
- **Button primary**: ink fill, on-ink text, square, 44 px tall; hover lifts 2 px with a 5 px accent offset shadow. **Button accent**: red fill, white text; hover shadow is ink. **Button outline**: transparent with a 2 px ink border.
- **Chip / filter**: 2 px ink border, transparent; active = ink fill.
- **Cover**: 3:4.3 ratio, 3 px ink border; hover moves it −4 px/−4 px with an 8 px accent offset shadow.
- **Segment progress bar**: 20 blocks, 3 px gap, filled = ink, last filled block = accent.
- **Reading card stack**: three stacked cards (accent, grey, paper), fanned out on hover; mono session table; rotated "IN PROGRESS" stamp in the accent color.
- **Catalog table**: ink header row with on-ink text, mono 12 px; row hover fills accent with white text; status tags OUT (accent), FINISHED, ON SHELF (outlined).
- **Speech bubble**: white oval, 3 px ink border, Anton uppercase.

## 5. Screens
1. **Home**
   - Top band split 1 : 1.3 with a 3 px divider.
     - Left: "NOW READING · TYPE · SERIES 001/096" (mono), giant "№07" with the number in the accent color, series title, mono meta line, 20-block progress bar, Resume and Open series buttons.
     - Right: **3D carousel** of in-progress series on a halftone and speed-line background, with prev/next buttons and a "1 / 7" counter.
   - The carousel **drives** the left column, the reading card and up-next: when it turns, the number, title and bar re-animate.
   - Mid row: three slanted panels: Reading card · Up next (next 3 volumes, or "All caught up + Import №NN") · Drop CBZ (inverted panel with burst).
   - Bottom: catalog table of series.
2. **Library**: Anton "LIBRARY", mono counts, filter chips (All, Manga, Manhwa, Reading, Unread, Finished), sort, Covers/Details toggle; 6-column cover grid; Details = catalog list.
3. **Series**: cover with 10 px accent shadow, title, Japanese title, chips, stats band (volumes, finished, pages, size), Resume and Add volumes buttons, volume grid (8 columns) with per-volume progress, files table (file, volume, size, pages, archive type, open time).
4. **Reader**: always uses the reader background. Top: back, Spread / Single / Long strip, title and page counter, Fit / 100% / more. Page frame: 4 px ink border with a 10 px accent offset shadow. Bottom: thumbnail strip (current page outlined in accent, cached pages lighter) and an RTL scrubber.
5. **Import**: drop zone with an animated dashed ink border (marching ants), detection rules box, "Keep files where they are" / "Copy into KOMA library" options, queue of detected files with series match, progress and steps (Read index → Cover → Thumbnails); unmatched files are flagged in the accent color.
6. **Settings**: reading direction, page-turn animation, theme and accent, reduce motion, cache size, preload range, thumbnail cache, library folders.

## 6. Motion
| Name | Duration | Easing | Used for |
|---|---|---|---|
| micro | 140 ms | `cubic-bezier(.2,.8,.2,1)` | hover, press |
| ui | 260 ms | same | tab and segment changes, cards rising in |
| page | 420 ms | same | screen changes, page turns |
| slam | 450–550 ms | `cubic-bezier(.3,1.3,.5,1)` | panels and titles slamming in, stamps |
| carousel | 700 ms per step; auto-advance every 3.6 s | `cubic-bezier(.2,.8,.2,1)` | 3D carousel (pauses on hover) |

- Carousel geometry: side cards at `translateX(d×175px) translateZ(−|d|×170px) rotateY(−d×30deg)`. Side cards are partly grayscale; the front card gets the accent shadow.
- Lists stagger in at 50 ms per item.
- Reduce motion: disable loops (carousel auto-advance, rotating bursts, marching ants) and shorten transitions to 0 ms.
