# Web UI design standard

This is the rulebook for changes under `web/`. It describes the shell, the state
model, the visual tokens and the checks that must pass before a UI change ships.
It is not a brand document and it is not frozen. Change a rule here first, then
change the code.

## Shape of the app

The page has four fixed regions:

1. A compact header with the title, the runs button (narrow screens only), Open
   files and the shortcuts button. Below 560 px Open files becomes icon-only
   with a visible folder icon, an accessible name and a tooltip.
2. A runs rail on the left. On wide screens it is a sticky column; at 900 px and
   below it becomes a drawer opened from the header. The rail is grouped by
   source file, with a Compare control on every run that is not selected.
3. The workspace on the right, containing the tabs, the comparison bar and the
   active panel page. The tabs stay at the top of the workspace while scrolling.
4. A transport pinned to the bottom edge. It is visible on every tab and in the
   empty state; the fullscreen 3D view gets its own playback bar instead.

The tabs are Overview, 3D path and Inputs. Their IDs are
`tabbutton-overview`, `tabbutton-3d`, `tabbutton-inputs`, with panels
`tab-overview`, `tab-3d` and `tab-inputs`. The runs rail and the transport sit
outside the tab pages. Switching tabs must never stop playback or clear a
selection.

Before a run is selected, the workspace shows one drop target with the accepted
file types and the promise that parsing happens locally. The tabs and panels
stay hidden until there is a run to inspect. Do not fill the empty state with
fake numbers.

## State model

A run is one ghost or one replay-level record decoded from a file. The
selection and the comparison set are separate:

- `selectedId` is the run driving the dashboard, charts, splits, live input,
  piano roll, TAS panel and 3D car.
- `compareIds` is an ordered list of extra runs. Entry zero is the reference
  for deltas and split comparisons. Every other entry is still drawn in the
  speed and 3D views.
- A run cannot be both selected and compared.
- Selecting a run that is already in `compareIds` swaps the old selection into
  that comparison slot. The user does not lose the run they were comparing
  against.
- Removing a run or a whole file re-runs auto-selection over what remains. A
  replay-level record is displayed as `nickname · record`; never show it as a
  numbered ghost.

After a load, `bestRun()` picks the most useful run: vehicle samples first, then
a finish time, then an input stream. This matters for merged fixtures where the
first ghost has inputs but no samples and the replay-level record has the
samples. Never replace it with an unconditional `runs[0]`.

The clock is shared by all panels. It holds `timeMs`, `playing` and `rate`.
Changing the selection resets the time and stops playback. Changing tabs does
not. `durationMs` is the recorded length. `raceTimeMs` is the finish time and
can be shorter than the recorded length.

The comparison bar is the only place that owns the comparison set. It reads
`Compare <selected> with`, then one option per compared run. Every option has a
radio for the reference, the run's colour, its display name, finish time and a
remove button; Clear empties the set and a `different maps` warning points at
the affected comparison. Do not put comparison state in a separate panel. Chart
and path legends name the selected run and the reference by role when the names
alone would be ambiguous.

## Controls

Use the helpers in `src/lib/ui.ts` for buttons and badges and the inline icons in
`src/lib/icons.ts`. Do not put glyphs such as triangles, arrows or crosses in
button text. An icon-only button needs an accessible name and a tooltip. A
visible text label is better when there is room.

Keep the 3D view actions distinct: Fit is a corner frame with a centre square,
Follow is a crosshair, Fullscreen is four outward corners, and the active
fullscreen state is four inward corners. Do not use a diagonal resize arrow for
fullscreen; it reads as a window handle, not a display mode.

One action per area gets the primary style. In the header it is Open files; in
the transport it is Play/Pause. Everything else is a normal or ghost button.
Disabled controls stay in place so the layout does not jump.

Keyboard rules:

- Tabs use `aria-selected`, a roving `tabindex`, and Left, Right, Home and End.
- The runs drawer opens from the header, traps nothing, closes on Escape or a
  run selection, and always returns focus to the header button, whether it was
  closed by its close button, Escape or a selection. A closed drawer is inert
  and must stay out of the tab order; the static rail above 900 px is never
  inert.
- Controls that rebuild their list keep focus on the replacement: the selected
  run button, the Compare checkbox and the chosen reference radio. Losing focus
  to the page body is a bug, not a harmless re-render.
- The shortcuts dialog is a native `<dialog>` with a visible close button and
  lists the `?` shortcut itself.
- Space toggles playback, arrows step 0.1 s, Shift plus arrows step 1 s, Home
  and End jump to the ends, F toggles 3D fullscreen, `?` opens the shortcuts,
  and Escape closes the dialog, drawer or fullscreen.
  Global shortcuts ignore events from fields, buttons, links, open dialogs and
  the piano roll, which owns its arrow and zoom keys.
- The transport range, rate select and comparison radios work with the keyboard
  without extra handlers.

The piano roll supports click or drag to seek, wheel to zoom, Shift-drag to pan,
double click to fit, and arrow, plus, minus, 0, Home and End when focused. On a
coarse pointer, show the touch wording (drag to seek, Fit and ± to zoom) instead
of keyboard and wheel instructions.

A run row shows its finish and either its sample count, its input tick count when
there are no samples, or `no data`. Keep the `no samples` warning badge, but do
not repeat the same phrase in the meta line.

## Visual system

All colours, radii, shadows and shell dimensions live in
`src/styles/tokens.css`. Panels use `--surface-1`, controls use `--surface-2` or
`--surface-3`, borders use `--border` and `--border-strong`, and the accent is
`--accent` with `--accent-surface` and `--accent-border` for selected states.
Semantic tones are `--ok`, `--warn`, `--danger` and their surface and border
variants. Do not add raw hex colours in panel code when a token exists. A run's
palette colour is data, not chrome, so it is allowed in a run swatch or chart.

The 3D speed mode uses one adaptive km/h range across the visible runs and
shows that range with a ramp swatch in the legend. Never hard-code a speed
ceiling: a fast map collapses into one endpoint colour. Values outside the
robust range clamp to the endpoint colours. Draw the selected run's path after
the dimmed references so its speed colours are not tinted by a reference's run
colour.

Text sizes are 14 px base, 12.5 px for controls, 11.5 px for secondary copy and
10.5 px for labels and metadata. Numbers use `--mono` and tabular figures.
Buttons are 30 px high. Panel headers are 36 px. The header is 56 px high and
the transport 84 px on wide screens; below 560 px they are 52 px and 132 px
high. Keep those numbers in sync with the CSS custom properties instead of
repeating magic numbers.

The stylesheets load in this order from `src/styles/index.css`: tokens, base,
components, shell, panels. Put reusable controls in `components.css`, layout and
responsive shell rules in `shell.css`, and panel-specific rules in `panels.css`.
There is no CSS framework and there are no external assets.

Interaction states are not optional: hover, `:focus-visible`, active/selected and
disabled. Focus rings use the accent and must remain visible on dark surfaces.
Respect `prefers-reduced-motion`. Keep horizontal overflow out of the viewport
at 390 px; wrap controls instead of clipping them.

## Words

Use these terms consistently:

- Run, not ghost, unless the file format distinction matters.
- Selected run or current run for the run driving the panels.
- Comparison or compared run for an extra overlay.
- Reference for the comparison entry that drives deltas and split deltas.
- Finish time for the timer, recorded length for the whole sample stream.
- Checkpoint and split. A run's last checkpoint is the finish line.
- Input stream for the 100 Hz events, vehicle samples for the 20 Hz car state.
- No samples and no inputs are different states. Say which one is missing.
- Different maps is a warning, not an error. Distance deltas still draw.

Status messages are short and specific. Errors name the file and the failure.
They appear in dismissible toasts that auto-hide, sit below the tabs on narrow
screens and never cover the transport. Parse warnings belong to the run's
warning box, not to the global toast.

## Required checks

A passing type check and unit tests do not prove the UI works. Before reporting a
frontend change:

1. Run `npm run check`, `npm test`, `npm run smoke` and `npm run build` in `web/`.
2. Run `npm run shots` and open every generated image. The script writes to
   `web/.shots/`, which is gitignored. It uses viewport-sized captures plus
   scroll-to-panel captures, and covers the empty state, both runs in the merged
   fixture, the no-samples 3D state, the single sampled run, single and multiple
   comparisons, every tab, the 3D fullscreen player, the narrow-screen drawer and
   the shortcuts dialog at 1440, 1024 and 390 px.
3. Check for missing glyphs, clipped labels, horizontal overflow, blank panels,
   content hidden behind the fixed transport, and a broken 3D canvas when WebGL
   is available. Do not use full-page captures for this; sticky tabs and fixed
   controls move away from their real positions.
4. Repeat the file load, run selection, reference change, tab switch, playback,
   fullscreen and drawer close with the keyboard only.
5. Read the browser console for errors from every capture pass.

`npm run shots` starts Vite and Playwright. On a machine where Playwright's
browser download is missing, install it with `npx playwright install chromium`
or point `SHOTS_CHROMIUM` at an existing Chromium binary. Use `SHOTS_BASE_URL`
to reuse a running dev server, and `SHOTS_OUT` to change the output directory.

When a change affects the shell, a shortcut, a tab name or the state model,
update this file and `README.md` in the same change.
