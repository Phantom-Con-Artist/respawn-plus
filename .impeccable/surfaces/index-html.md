---
version: 1
slug: "index-html"
primary_target: "index.html"
related_targets: ["src/main.js","src/styles.css"]
---

# Launcher app (index.html, src/)

Scope: the whole Vessel desktop launcher: Library (first screen), game page (game dashboard, play setup, companion apps, Room tab), Dependencies, Settings, the launch screen and the boot screen. Mode: Operate.

Audience and job: mod-curious Linux players choosing how to play a game (vanilla, with a trainer, through a mod manager, mod manager plus trainer) and pressing Play; secondary: power users adjusting the room. Must stay keyboard friendly (search focus, Enter to play, visible focus). Library is home; each game page carries that game's dashboard (playtime over 14 days, totals, sessions). Room settings live in a Room tab.

Constraints: Swiss International Style; palettes Milky (light), Night (dark), Ashen (grey) plus the logo cyan and deep teal; square controls, no pill buttons; no glass, no morphism; boring and easy. Agnostic: setup follows companion kinds only.

## Direction contract

THESIS: A game's play setup is a strip of square blocks, read left to right, and Play runs exactly that strip. Refuses the category default of a cover grid plus a settings dump where how a game starts is hidden in a dropdown.

OWN-WORLD: Swiss grid on flat fields: Milky warm-neutral white, Night teal-black, Ashen mid grey; one grotesk, flush left, hierarchy by scale contrast; 2px corners everywhere; colour reserved: cyan only on the active route (Play, the running strip, on-states), deep teal as ink on cyan; tone steps, never shadows.

STORY: The player opens Vessel, finds the game, sees its strip (what starts it, what opens with it), toggles a trainer slot on or off, presses Play, and watches the same strip fill block by block until the game is up.

FIRST VIEWPORT: Library: left rail (mark, Library, Dependencies, Settings; running rooms listed below), search field at top of the canvas, games as a flush-left list of rows with art thumb, name, setup summary, last played and Play. Game page: art band, name 24px, then the room strip at full canvas width with Play at its right end, tabs below (Dashboard, Companion apps, Room).

FORM: Room strip, candidate 7 of 7 (console memory-card save blocks), seed:c43c99f03388dee7. Raises: from the orienteering map, cyan only on the active route; from the developer console, destructive actions isolated and outlined; from the catalog sleeve, no state labelled twice; from the boxed manual, changes step in two frames, no drifting easing; from the specimen, hierarchy by scale alone.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
