---
name: Vessel
description: A Linux launcher where each game's play setup is a strip of square blocks, and Play runs exactly that strip.
colors:
  cyan: "#18e6e4"
  on-cyan: "#0b1c1f"
  milky-ground: "#f3f2ee"
  milky-surface: "#fbfbf9"
  milky-raised: "#e8e7e1"
  milky-line: "#dcdad3"
  milky-control: "#b9b7ae"
  milky-text: "#141818"
  milky-muted: "#4f5553"
  milky-faint: "#636967"
  milky-negative: "#b4232a"
  milky-mark-deep: "#0b1c1f"
  night-ground: "#0a1012"
  night-surface: "#0f171a"
  night-raised: "#172124"
  night-line: "#1f2b2e"
  night-control: "#3b4a4f"
  night-text: "#edf2f1"
  night-muted: "#aab5b3"
  night-faint: "#8d9997"
  night-negative: "#f07178"
  night-mark-deep: "#2f6f74"
  ashen-ground: "#4a4d4c"
  ashen-surface: "#545756"
  ashen-raised: "#5f6261"
  ashen-line: "#6a6d6c"
  ashen-control: "#a8abaa"
  ashen-text: "#f4f5f4"
  ashen-muted: "#e0e2e1"
  ashen-faint: "#cfd2d1"
  ashen-negative: "#ffb3b3"
  ashen-mark-deep: "#1d4e52"
typography:
  title:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 650
    lineHeight: 1.1
    letterSpacing: "-0.02em"
  figure:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "24px"
    fontWeight: 500
    letterSpacing: "-0.01em"
  play:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 650
    letterSpacing: "-0.01em"
  headline:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "-0.01em"
  lead:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.45
  body:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  block-name:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
    lineHeight: 1.25
  label:
    fontFamily: "Hanken Grotesk Variable, Hanken Grotesk, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.3
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, DejaVu Sans Mono, monospace"
    fontSize: "13px"
    fontWeight: 400
rounded:
  hair: "1px"
  base: "2px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  gap: "16px"
  lg: "20px"
  xl: "24px"
  section: "28px"
  pad: "40px"
components:
  button:
    backgroundColor: "{colors.milky-surface}"
    textColor: "{colors.milky-text}"
    rounded: "{rounded.base}"
    padding: "0 14px"
    height: "36px"
  button-hover:
    backgroundColor: "{colors.milky-raised}"
  button-primary:
    backgroundColor: "{colors.milky-text}"
    textColor: "{colors.milky-ground}"
    rounded: "{rounded.base}"
    padding: "0 14px"
    height: "36px"
  button-primary-hover:
    backgroundColor: "{colors.milky-muted}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.milky-negative}"
    rounded: "{rounded.base}"
    padding: "0 14px"
    height: "36px"
  button-go:
    backgroundColor: "{colors.cyan}"
    textColor: "{colors.on-cyan}"
    rounded: "{rounded.base}"
    height: "36px"
  play:
    backgroundColor: "{colors.cyan}"
    textColor: "{colors.on-cyan}"
    typography: "{typography.play}"
    rounded: "{rounded.base}"
    size: "112px"
  play-disabled:
    backgroundColor: "{colors.milky-raised}"
    textColor: "{colors.milky-faint}"
  block:
    backgroundColor: "{colors.milky-ground}"
    textColor: "{colors.milky-text}"
    rounded: "{rounded.base}"
    padding: "8px"
    size: "112px"
  block-on:
    backgroundColor: "{colors.cyan}"
    textColor: "{colors.on-cyan}"
  block-off:
    backgroundColor: "transparent"
    textColor: "{colors.milky-text}"
  strip:
    backgroundColor: "{colors.milky-surface}"
    rounded: "{rounded.base}"
    padding: "20px"
  panel:
    backgroundColor: "{colors.milky-surface}"
    rounded: "{rounded.base}"
    padding: "20px"
  input:
    backgroundColor: "{colors.milky-surface}"
    textColor: "{colors.milky-text}"
    rounded: "{rounded.base}"
    padding: "7px 10px"
    height: "36px"
  search:
    backgroundColor: "{colors.milky-surface}"
    textColor: "{colors.milky-text}"
    typography: "{typography.lead}"
    rounded: "{rounded.base}"
    padding: "0 12px"
    height: "44px"
  nav-item:
    textColor: "{colors.milky-muted}"
    rounded: "{rounded.base}"
    padding: "0 10px"
    height: "34px"
  nav-item-active:
    backgroundColor: "{colors.milky-raised}"
    textColor: "{colors.milky-text}"
  tab:
    textColor: "{colors.milky-muted}"
    height: "40px"
  tab-selected:
    textColor: "{colors.milky-text}"
---

# Design System: Vessel

## Overview

**Creative North Star: "The Memory-Card Strip"**

Vessel reads like a console's save screen set in Swiss type: a game's setup is a row of square blocks, read left to right, and the cyan square at the end runs exactly what the row shows. Everything around the strip is a flat field in one grotesk, flush left, ruled by 1px lines. The system is quiet on purpose so that the one saturated colour can mean one thing: this is chosen, or this is running.

Density is that of a desktop tool, not a storefront. The Library is a ruled list of rows, not a cover grid; the game page puts the strip at full canvas width above three tabs (Dashboard, Companion apps, Room). Three palettes, Milky, Night and Ashen, share one structure and swap only values. Depth comes from tone steps and lines, never from shadow; corners are 2px everywhere; state changes step in two frames instead of easing.

The logo mark is two folded panels, cyan and a palette-specific deep teal. It is the only illustration in the interface besides game art, and it is the one thing that animates while something is starting.

**Key Characteristics:**
- One grotesk (Hanken Grotesk Variable), flush left, weights 400 to 650.
- Three flat palettes on one set of role tokens; cyan shared across all three.
- Cyan only on chosen or running things; everything else is text colour on ground.
- 112px square blocks as the signature unit; Play is the same square.
- 2px corners, 1px lines, no shadows.
- Two-frame stepped motion; reduced motion removes it all.

## Colors

Flat neutral fields in three temperatures, warm white, teal-black and mid ash, with a single electric cyan that is the same in every palette.

### Primary
- **Live Cyan** (`cyan`): the logo's light panel and the only accent. Fills Play, a selected or switched-on block, a Library row's Play button, a finished launch step, the running dots in the rail and header, the live mini-strip, and text selection.
- **Deep Teal Ink** (`on-cyan`): text and icons on cyan. Never used on its own as a field.

### Neutral
Each palette defines the same roles. Milky is the light default; Night is teal-black; Ashen is a mid ash grey that is neither light nor dark mode.
- **Ground** (`milky-ground`, `night-ground`, `ashen-ground`): the window, the rail and the inside of a block.
- **Surface** (`*-surface`): one step up from ground. The strip tray, panels, inputs, buttons, dialogs, the figures band.
- **Raised** (`*-raised`): hover fill, the current nav item, art placeholders, notices, disabled Play.
- **Line** (`*-line`): every 1px rule and container border, the tab rule, row dividers, chart grid lines.
- **Control** (`*-control`): borders of things you can operate (buttons, inputs, blocks), dashed borders, scrollbar thumb.
- **Text** (`*-text`): body text, and also the focus ring, caret, links, checked radios and the selected tab underline.
- **Muted** (`*-muted`): secondary lines, labels, inactive nav and tabs, chart bars.
- **Faint** (`*-faint`): placeholders, counts, disabled text, chart ticks, the version foot.
- **Negative** (`*-negative`): errors, missing executables, the attention state, outlined destructive buttons, the close button hover.
- **Mark Deep** (`*-mark-deep`): the logo's second panel only, tuned per palette so the mark holds on each ground.

### Named Rules
**The Live Cyan Rule.** Cyan marks what is chosen or running, nothing else: a selected or switched-on block, Play, running dots, a finished launch step. Headings, links, focus, hover and decoration never use it.

**The Text-Colour Focus Rule.** Focus rings (2px solid, 2px offset), the caret, links, native radios and checkboxes, and the selected tab all use the text colour, so cyan keeps its single meaning.

**The One Structure Rule.** A palette changes values, never roles or layout. A new palette defines all ten role tokens and keeps cyan and deep teal ink as they are.

## Typography

**Display Font:** Hanken Grotesk Variable (with Hanken Grotesk, system-ui, sans-serif)
**Body Font:** the same family
**Label/Mono Font:** ui-monospace, SFMono-Regular, DejaVu Sans Mono, used only for file paths and logs

**Character:** One neutral grotesk carries every role, tightened slightly at the larger sizes (-0.01em to -0.02em). Hierarchy comes from size and weight steps, not from colour or case.

### Hierarchy
- **Title** (650, 28px, 1.1): page names (Library, a game's name, Settings) and the launch screen heading.
- **Figure** (500, 24px): the dashboard numbers (total playtime, last 7 days, sessions).
- **Play** (650, 20px): the Play label inside its square.
- **Headline** (600, 18px, 1.3): panel heads and setting labels (Runner, Display, Appearance).
- **Lead** (16px, 1.45): sublines under titles, Library row names (600), the setup sentence under the strip, the search input, the launch note.
- **Body** (400, 14px, 1.45): everything else. Setting descriptions cap at 40ch, panel intros at 62ch.
- **Block name** (600, 13px, 1.25): the name inside a block, two lines at most, balanced.
- **Label** (500, 12px): block kinds and states, strip group labels ("Start with", "Opens with the game"), table heads, the rail's group heading, fact terms.
- **Mono** (13px): file paths; logs at 12px/1.5.

Numbers that change or align use tabular figures.

### Named Rules
**The Sentence-Case Rule.** Every label is sentence case at its natural tracking. Nothing is uppercased or letter-spaced to look like a label.

## Layout

The window is a frameless shell on a two-column grid: a 232px rail on the left (mark, Library, Dependencies, Settings, then the running rooms, version at the foot) and the canvas on the right under a 40px bar that holds the status message and the window controls. Pages fill the canvas width with no maximum; the canvas has 40px side padding.

- **Library:** search field (44px) on top, then games as a ruled list. Each row is art thumb (128px, 16:9), name and setup summary, mini-strip, last played, and Play (104px column).
- **Game page:** back crumb, then a head of 16:9 art (224px) beside the title, meta line and state; then the strip in a surface tray at full width with Play at its right end; then the setup sentence; then tabs on a full-width rule.
- **Dashboard:** a four-cell figures band, then a 3:2 split of chart and sessions.
- **Settings and Room:** label column (up to 300px) beside controls (up to 560px), 40px apart, each setting ruled off at 28px rhythm.

Spacing runs on 4/8 with frequent 12, 16, 20, 24 and 28 steps; 16px is the default gap between fields.

Responsive behaviour:
- **From 1100px up**, the strip stays on one row.
- **At 1100px and below**, side padding drops to 28px, Library rows drop the mini-strip and time columns (thumb 112px), the dashboard stacks, and settings put the label above the controls.
- **At 760px and below**, side padding is 16px, the rail becomes a horizontal bar of nav links under the title bar (no icons, no running list), Library rows put Play full width under the name, figures go two by two, strip groups take full rows, and Play and Stop become full-width 72px bars.

## Elevation & Depth

Vessel is flat. There is no box-shadow for depth anywhere. Depth is a tone step (ground, then surface, then raised) plus a 1px line. The strip tray is one step above the window; blocks sit back on ground inside it; hover fills one step up. The dialog backdrop is the ground colour at 70%, not a dark scrim.

### Named Rules
**The Tone Step Rule.** To lift something, move it one tone step and give it a 1px line. Never add a shadow, blur or glow.

## Shapes

Every corner is 2px: buttons, inputs, blocks, Play, panels, thumbnails, dialogs, the focus ring's frame. Only sub-16px marks (mini-strip squares, theme swatches) drop to 1px. Dots are square. There are no pills and no circles.

Line style carries state: a solid control-colour border is something available; a dashed border is something absent or off (a companion switched off, the add slot, a pending launch step, an empty mini-strip square); a 2px text-colour border is the current one (the active launch step, the chosen theme).

### Named Rules
**The Square Block Rule.** The block is a 112px square (`--block`) and does not stretch. Play is the same square at the end of the strip, and while the room is open Stop room takes Play's square. A group with too many blocks wraps to another row of squares rather than shrink them.

**The Dashed-Means-Off Rule.** Dashed borders mean off, absent or not yet. Never use a dash as decoration.

## Components

### Buttons
Flat, square-cornered, one height.
- **Shape:** 2px corners, 36px tall, 14px side padding, 500 weight, 1px control border.
- **Default:** surface fill, text colour; hover fills one tone step (raised); active nudges down 1px.
- **Primary:** text-colour fill with ground-colour text; hover drops to the muted tone.
- **Go (Library Play):** cyan with deep teal ink, 88px minimum width; hover mixes 18% text into the cyan.
- **Danger:** transparent, negative text and outline; hover tints 10% negative. In tables it sits 16px apart from the other actions.
- **Link:** text colour, 500, underlined in the control colour, underline goes full strength on hover.
- **Disabled:** faint text, line-colour border, not-allowed cursor.

### Inputs / Fields
- **Style:** surface fill, 1px control border, 2px corners, 36px minimum height, 7px 10px padding. Labels sit above in 500 weight, 6px away.
- **Focus:** the global 2px text-colour ring at 2px offset. The search field instead turns its whole border to text colour.
- **Disabled:** raised fill, faint text.
- **Choices:** native radios and checkboxes at 16px, tinted with the text colour. Inputs joined to a button lose their shared corners and border.

### Navigation
- **Rail:** 34px rows with a 16px stroke icon, muted text; hover and current page fill raised with text colour. Counts sit at the right in faint tabular figures. Below, "Running" lists open rooms, each with a square cyan dot.
- **Tabs:** muted 500 text on a full-width 1px rule, 24px apart; the selected tab turns text colour with a 2px text-colour underline.
- **Window controls:** 46px wide, drawn from 1px lines; close hovers to a negative fill.

### Cards / Containers
- **Corner Style:** 2px.
- **Background:** surface on ground.
- **Shadow Strategy:** none; see Elevation & Depth.
- **Border:** 1px line colour.
- **Internal Padding:** 20px for panels and the strip tray; 16px 20px for figure cells, which are divided by 1px lines rather than gaps.

### The Room Strip (signature)
The game's setup as blocks in a surface tray, grouped under 12px labels: "Start with" (the game and any app that can start it, a radio group) and "Opens with the game" (trainers and utilities, toggle buttons), then an add slot, then Play pushed to the right end.
- **Block:** 112px square on ground, 8px padding, three rows: kind (12px muted), name (13px 600, two lines), state (12px muted). Hover turns the border to text colour.
- **Chosen or on:** cyan fill and border with deep teal ink throughout.
- **Off:** dashed border, transparent.
- **Missing executable:** the state line turns negative.
- **Running:** a square 8px dot in the state line. The word "Running" appears once, in the game head's state line, not on each block.
- **Keyboard:** the start group is one Tab stop with roving tabindex; arrow keys move and choose.
- **Under the strip:** one 16px sentence says what Play will do in order.

### Launch Steps
The same squares fill in sequence on a full-window launch screen: pending steps are dashed and muted; the active one has a 2px text border and blinks between ground and raised in two frames (720ms); a done step goes cyan; a failed step takes a 2px negative border and negative state text.

### Theme Picker
A radio group of four cards (Match system, Milky, Night, Ashen), each a three-band swatch above its name. The chosen card takes a 2px text-colour border. Same roving tabindex and arrow keys as the strip.

### Motion
State changes (background, border, colour) step in two frames: `steps(2, jump-none)` at 90ms. While something is starting, the mark's two panels blink in turn (720ms, offset 360ms). The boot screen leaves with a 180ms two-step fade. `prefers-reduced-motion: reduce` turns off every animation and transition.

## Do's and Don'ts

### Do:
- **Do** keep cyan for what is chosen or running: Play, a selected or switched-on block, running dots, finished launch steps.
- **Do** draw focus rings, the caret and links in the text colour (2px ring, 2px offset).
- **Do** build every new control at 36px with 2px corners and a 1px control border.
- **Do** make anything in a setup a 112px square block, and give Play (or Stop room) the same square.
- **Do** lift with one tone step and a 1px line.
- **Do** step state changes in two frames (`steps(2, jump-none)`, 90ms) and honour reduced motion.
- **Do** say a state once, in the place that owns it.
- **Do** give radio groups one Tab stop with arrow-key movement.
- **Do** define all ten role tokens when adding a palette, and keep cyan and deep teal ink unchanged.

### Don't:
- **Don't** use cyan for headings, links, hover, focus or decoration.
- **Don't** add box-shadows, glows, blur or glass for depth.
- **Don't** round corners past 2px, and don't make pills or circles.
- **Don't** use smooth easing curves or drifting transitions.
- **Don't** use dashed borders for anything that is not off, absent or pending.
- **Don't** repeat a state label on every element it applies to.
- **Don't** uppercase or letter-space labels, or add small labels above titles.
- **Don't** introduce a second typeface; mono is only for paths and logs.
- **Don't** use pure black or pure white; every palette's text and ground are tinted.
