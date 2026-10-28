# Nagare

The PlayStation 3 VSH background, reverse-engineered, and slightly enhanced, as a live desktop wallpaper: a Rust port of
`ps3xmbwave/`, the same wave, backdrop, post-processing and particles, on OpenGL 3.3 or Vulkan with an
[egui](https://github.com/emilk/egui) settings panel. It is a rusty desktop port of the breakthrough research of
[faquino/PlayStation-3-XMB](https://github.com/faquino/PlayStation-3-XMB) and
[linkev/PlayStation-3-XMB](https://github.com/linkev/PlayStation-3-XMB) - without them, Nagare would be nada! Nagare (流れ, a flow) is the product's name;
Sony's names are used only to say what it recreates. The executable is `nagare.exe` (the crate `nagare`). On Windows it
starts as a live desktop wallpaper, behind the icons, with a tray icon to control it; elsewhere, and with `--window`,
it is a window.

```bash
cargo run --release                          # behind the desktop icons (Windows), with a tray icon
cargo run --release -- --window              # a window, with the settings panel
cargo run --release -- --fps 60              # capped at the console's 60 frames a second
cargo run --release -- --window --perf-overlay 3   # with the perf overlay and its graph
cargo run --release -- --renderer opengl     # drawn with OpenGL (Vulkan is the default)
```

| Option             | What it does                                                                               |
| ------------------ | ------------------------------------------------------------------------------------------ |
| `--wallpaper`      | Start behind the desktop icons (the default on Windows).                                    |
| `--window`         | Start as a window instead.                                                                  |
| `--no-ui`          | Start the window with the settings panel hidden.                                           |
| `--fps N`          | Draw at most N frames a second (6 at the least; by default the benchmark's choice, 60 or 30). Over the config's cap, for this run; the panel's Frame rate still changes it. |
| `--perf-overlay N` | Show the perf overlay at level N (1 to 3).                                                  |
| `--renderer API`   | Draw with `vulkan`, `opengl` or `software` this run, over the Resources tab's renderer, which the config keeps. |
| `--no-watchdog`    | Start no watchdog (below).                                                                  |
| `--ae`             | Start with auto exposure on (saved, like the panel's AE button).                            |
| `--settings`       | Start with the settings open (their own window, behind the icons).                          |
| `--tab NAME`       | Start the settings on a tab (Scene, Input, Backdrop, Wave, Post, Particles, Panel, Resources). |
| `--no-save`        | Read the config but never write it.                                                         |
| `--first-start`    | Play the first start's intro this run: the benchmark, the epilepsy warning and the card (also under `--test`). |
| `--config FILE`    | Use another config file for the run.                                                        |

`--test` and the options that go with it are under [Testing and profiling](#testing-and-profiling); `nagare --help`
lists everything.

- **H** toggles the panel. The arrow keys send D-pad steps, and dragging shakes the virtual Sixaxis, as on the page.
- **Poking** the scene - clicking it quickly, `pokesToOpen` times (9), no more than 0.8 s apart and all within the poke
  shield's radius (50 points) of the first - opens the settings; a click further off starts the count again, and a
  press dragged out of that radius ends it, so clicking and dragging about the desktop never opens them.
  Each poke bounces the camera forward and back, half of `pokeBounce` (0.09) at the first and all of it from halfway
  on, and from the third on a hint says how many more it takes. Behind the icons, poke the desktop itself - a click
  on an icon is the icon's, and breaks the run (the icons' list view reports the item under the pointer as its hot
  item): the settings open in a window of their own, centred on the screen, while the scene goes on behind. Both are on the Panel tab, under Opening the settings; 0 pokes turns it off.
  The bounce moves only the drawing - the wave's clip coordinates and the particles' view - never the simulations.
  From 30 s into the XMB's start, a small tip at the top of the scene says so ("Tip: poke (click) the background 9
  times to show the settings.", with `pokesToOpen`'s count), whenever the settings, the intro and the epilepsy warning
  are not showing; clicking its × - in a window, or on the desktop behind the icons - or opening the settings by
  poking puts it away for good (the config keeps `pokeTipDismissed`). A `--test` run shows none.
- **The poke shield** (`shield.rs`): the clicks still coming as the settings open by poking must not land on a knob.
  So for a second the settings refuse clicks within 50 points of where the pointer was as they opened. A refused
  click shows the shield - a circle, its place and size the very ones it refuses clicks in, bouncing, with "Click-block
  activated" - and keeps refusing for 2.5 s from that click, the circle fading out as the time runs down; each refused
  click starts the time again. The pointer leaving the circle ends it at once, the circle swelling as it fades away
  over a quarter of a second. Clicks elsewhere go through.
- **The scroll hint**: at the foot of the panel, "Scroll down to see more knobs!" and a bouncing arrow on a plain dark
  pill, bobbing gently, while there are knobs below. The first scroll down, by the wheel or the bar, shows the user
  knows, and puts it away for good (the config keeps `scrollHintDismissed`).
- **One Nagare at a time** (`single.rs`): a second Nagare started with the same config - the Start menu's shortcut
  clicked again, say - asks the running one to show its settings, and exits. Another config (`--config`) is another
  Nagare, and `--test` runs are never held back.
- **Streamer mode**: while a screen recorder or streaming program runs - OBS (`obs64.exe`, `obs32.exe`), Streamlabs,
  XSplit, Twitch Studio, vMix, Bandicam, Camtasia's recorder or Meld Studio, as `neighbour.rs`'s `RECORDERS` names
  them, looked for every two seconds - the panel hides the GPU's model behind a button, "Streamer mode ON: Click here
  to reveal your GPU model", which shows it until the recorder quits. There is no telling for sure that the screen is
  recorded; the program running is the sign taken.
- With `logoOnRightClick` (off by default), **right-clicking** the scene, or the desktop behind the icons, shows the
  logo just above and left of the pointer (the desktop's own menu opens below and right of it); clicking it opens the
  settings.
- Behind the icons the mouse plays with the particles as in a window: moving over the desktop pushes them, dragging
  shakes the virtual Sixaxis.
- The **tray icon** is put on the taskbar rather than in its overflow on the first start of each copy of the app: once
  Explorer has seen it, the app sets its `IsPromoted` under `HKCU\Control Panel\NotifyIconSettings`, unless the user
  has already chosen either way, and records the exe's path in `tray-pinned.txt`, in the config's folder. After that
  it is left where the user puts it, in Settings > Personalization > Taskbar or by dragging it. Its menu has **Settings**, **Toggle wallpaper** and **Exit**. **Set as wallpaper** (in the panel) and
  **Ctrl+Alt+Shift+W** toggle too. Going behind the icons covers the monitor the window is on; coming back, and
  exiting, restore the normal wallpaper. A window that starts as the wallpaper is never shown before it is there, and
  is shown there only once its first frame has been presented into it, which a hidden window keeps: shown at once, it
  covered the desktop in white until that frame came (made see-through until then instead, it still flashed white).
- Release builds have no console window. The window's title shows the arguments it was launched with, as do the
  panel and the perf overlay (level 2 and up).

## Installing (Windows)

`cargo xtask installer` builds the release exes and packages them with [Inno Setup 6](https://jrsoftware.org/isinfo.php)
(`winget install JRSoftware.InnoSetup`) into `target\installer\nagare-setup-VERSION.exe`; `--no-build` packages the
exes already built, a PGO build's say, and `--install` then installs it silently over the installed copy. The script is
`installer\nagare.iss`, and [installer/INSTALLER.md](installer/INSTALLER.md) has the whole story: building, silent
installs, and how the wizard's look is made. The installer:

- installs for the user alone, with no administrator, into `%LOCALAPPDATA%\Programs\Nagare`, where Windows keeps
  per-user programs: `nagare.exe` and `nagare-watchdog.exe`, a Start menu shortcut, a desktop one if asked, and an
  entry in Settings > Apps;
- offers **Start Nagare when I sign in**, ticked, and to start it when it is done;
- stops a running Nagare first, the watchdog before the app, and the uninstaller does the same; a silent install
  starts again the Nagare it stopped;
- on uninstall removes all of that, the start at sign-in too however it was set, and leaves the config, in
  `%APPDATA%\Nagare`, which is the user's.

It wears the Installer icon tab's box, and its wizard - light, dark or as Windows is, with a shot of the scene behind
its pages - is the Installer tab's (see [The customizer](#the-customizer)). The exes are not signed, so SmartScreen
warns about a downloaded installer until they are.

**Start with Windows** is at the panel's top, and only for an installed copy - one whose folder an entry in Settings >
Apps names as its install location - so that a sign-in never starts a copy that has moved, a build's say; elsewhere it
is greyed out. It is never asked for unprompted. It writes the user's own `Run` value, `Nagare`, under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; Task Manager's Startup apps can turn that off, which the panel
shows as off, and turning it on in the panel turns it back on there too.

## Software Update

Nagare's only use of the network (`update.rs`, Windows only). Nothing is sent until the user agrees: a card on the
scene, 8 s into the XMB's start, says that Nagare checks GitHub for a newer version every day or two and downloads it,
then asks before installing it, and that nothing about the user or the PC is sent. Its **OK**, or closing it, turns the checks on, and **No, don't
check** turns them off. Its buttons are caught as the poke tip's × is, behind the icons too. The panel's top row ends
with **Software Update:**, the channel (**Stable**, the default, or **Latest release**: `updateChannel`, also under
Resources > Software Update), an **Auto-check** box (which answers the card's question too), **Check now** and
**Reinstall**, with a line under it saying what the updater did last, and when it next checks. The config keeps
`updateConsent`, `updateAuto` and `updateNext`.

**Reinstall** forces a download and install of the channel's version whatever this copy is: the same version, to repair
an install, or even a newer one, to go back to that release. It goes through the same checks as any update (the
release key's signature, the installer's size and SHA-256), and, like them, installs only on a copy the installer
installed.

- **When.** A run's first check comes a random 1 to 60 minutes after the start, but never before the time the last run
  set. After a check comes a random wait of 24 to 48 hours, or 2 to 6 after a failure.
- **What.** The tag in the repository's `update/stable.txt`, read from GitHub, names the release to update to
  ([update/README.md](../update/README.md)). On *Latest release* it is the newest release published (no draft, no
  pre-release) instead: a HEAD request to `github.com/bungholesurfers/Nagare/releases/latest`, whose redirect to
  `.../releases/tag/vX.Y.Z` carries the tag - only the redirect is read, no page, a few hundred bytes, on a host the
  updater already uses and outside the API's rate limits. Each release carries `nagare-update.json`: the version, the installer's
  name, its size and its SHA-256. It is signed with Ed25519 by the release key, the release workflow's secret
  `NAGARE_UPDATE_KEY`, whose public half is in `signing.rs`. The manifest is believed only if that key signed it, it
  names the tag's version and an installer named for it, and that version is newer than this one. The installer is
  kept only if its size and SHA-256 match. A tag naming an older version does nothing, so the unsigned pointer can at
  worst hold updates back.
- **How.** HTTPS through rustls (ureq), to GitHub's hosts alone (`github.com`, `raw.githubusercontent.com`, and the
  storage release downloads redirect to). Every redirect is checked the same way, so nothing ever falls back to plain
  HTTP, as HSTS ensures in a browser.
  - Certificates are checked against Mozilla's roots, bundled, not the system's.
  - Timeouts, and a cap on each body's size.
  - The certificates are deliberately not pinned. GitHub changes them, and their authorities, without notice, and a
    pinned updater would then stop updating, including the update that would fix it. The signature is what makes a
    download trustworthy, whatever happened to the connection.
  - The requests carry only a User-Agent naming Nagare and its version.
- **Installing, when you say so.** The installer is downloaded into `%TEMP%\nagare-update`, as VS Code's per-user
  setup downloads its own into `%TEMP%`, and checked. Nothing is installed without the user's word:
  - **A card on the scene**, with the installer's glass box, says that version X is downloaded and checked, its size,
    and that installing closes Nagare for a few seconds and keeps the settings. It offers **Install now**, **Later**
    (or ×: no more card this run; the next check brings it back) and **What's new** (the release's page, in the
    browser). Its buttons are caught as the question card's are.
  - **The panel's top** says the same, with **Install now** and **What's new** on the status line.
  - **The tray icon's menu** gains an item at its top while an update waits - *Install Nagare 0.1.3*, or for a copy that
    cannot install it *Nagare 0.1.3 is out: open its page* - and a notification tells of it once a run, the installer's
    box beside its words.
  - **`updateAutoInstall`** (Resources > Software Update, off by default) installs by itself instead, once nothing is
    full screen, with no card.

  Installing runs the installer silently; it stops Nagare, installs over it and starts it again.
- **Portable copies.** A copy that was not installed (a build, or the portable zip) checks the same way, but never
  downloads the installer: the card says that version X is out and opens its release page, and the panel's status line
  links it.
- **Loop guard.** Before an installer runs, `tried.txt` beside it names its version. A Nagare still older than that
  afterwards knows the install did not take, and Auto-check leaves that version alone (Check now can still try it).
  Once Nagare is that version, the folder is cleared away; the uninstaller removes it too.

To test it without publishing anything:
- `NAGARE_UPDATE_SOURCE=FOLDER` reads `stable.txt` (or, on *Latest release*, `latest.txt`), the manifest, its signature and the installer from a folder.
- `NAGARE_UPDATE_NOW=1` checks 5 s after the start, as if the card had been answered OK (with `--no-save`, nothing of it
  is kept); `NAGARE_UPDATE_NOW=force` makes that check a forced one, as Reinstall does. Neither variable is passed on
  to the installer.
- `--update-fetch URL` fetches a URL exactly as the updater does, and says what came.
- `cargo test --release -- --ignored tls` checks that the client refuses expired, wrong-name, self-signed and
  unknown-authority certificates (it goes to the network).

`nagare-release` (`src/bin/release.rs`) is the release workflow's side: `keygen` makes a key, and `manifest` writes and
signs a release's manifest, checking that the app knows the key.

## Wallpaper mode: the mouse and the settings

Behind the icons the scene never gets the mouse's messages: the desktop's icons window (`SHELLDLL_DefView`) takes
them. So the scene reads the mouse itself (`wallpaper.rs`): each frame the pointer's position and the physical
buttons (swapped for a left-handed mouse), for its moves and drags, used only while the desktop is what is under the
pointer - not another window, the taskbar or a menu - and handed on as the window's own events would be; and the
clicks as they happen, from the mouse's raw input on a thread of its own, with where the pointer was at that moment.
So the logo shows where the right button went down, whatever the pointer did before it came up and however low the
frame rate, and no click is missed.

The panel itself is not drawn on the wallpaper: every click on it would land on the desktop too (dragging a slider
would draw a selection box over the icons), the keyboard never reaches it, and the icons would cover it. Instead poking
(or the logo) opens it in a settings window (`settings_window.rs`) of its own: an ordinary window, with the mouse and the
keyboard, sharing the scene's OpenGL context or Vulkan device and drawn after the scene's frame without waiting for
the display, so the scene's frame rate holds. **Back to a window** in it brings the scene back as a window, with the panel inside.

## Config

What you change is saved on its own, half a second after it changes, and loaded at start-up:

| Platform | File                                                   |
| -------- | ------------------------------------------------------ |
| Windows  | `%APPDATA%\Nagare\config.json` (the roaming AppData) |
| macOS    | `~/Library/Application Support/Nagare/config.json`     |
| Others   | `$XDG_CONFIG_HOME/nagare/config.json`, `~/.config/...` when unset |

The app kept it in a folder named `ps3xmb` before it was named Nagare: found where the new folder is not, it is
renamed to it, once. Settings that were renamed (`aeMaxBoost` to `aeMaxPush`, `blurWindowBackdrop` to
`windowBackdrop`, and the others in `settings.rs`'s `RENAMED`) are read under their old names too, and saved
under the new.

The file is written by a thread of its own, at the lowest priority, so that the disk never holds up a frame; the
newest text waits there, and the last of it is written before the app exits. The same goes for the app's other
waiting work: the watchdog's check at start-up and the tray icon's pinning run on such threads too.

It keeps the values changed in the panel (the amber ones) and the locks, the scene's state (theme, music, Theme
Settings, display, palette) and the tab - not every value, so the theme goes on writing the rest. Reset a value and it
is no longer kept; delete the file to start afresh. A file that cannot be read is moved aside to `config.json.bad`.
The panel shows the path, the home folder written as `%APPDATA%` or `~`, and **Show in folder** opens it in the file
browser. Not saved: the time override, the perf overlay's level and the panel's visibility, which the command line
sets. Saved besides the panel's values: the frame cap and the Resources tab's list of programs.

## The panel's top

At the very top, the app's name, what it is, its home on GitHub (github.com/bungholesurfers/Nagare), and whose research
it stands on, with links. Every link in the settings asks first: a prompt shows its address, with **Yes** (open it in
the browser), **Copy to clipboard** and **No** (Escape, or a click beside it, too). Clicking the name brings its
gags: the first time ever, in order - a Naga-reeee that grows, Nagheenanajar, Naga... ...Naganna work here anymore! HAH, Nagatoro,
a legally distinct spline mesh, then Claude Code's spinner words two a second, then the name again - and once that round
is done, or in any later run (the config keeps `titleRandom`), in rounds at random: every gag once, shuffled, then the
name again before the next round. Whenever the settings appear - the panel shown, or their window opened - the title
greets with a gag at random for 2 s, then slides up out of its line, on a sharp ease-in (slow to start, then quick),
as the name slides up into it from below. Above the tabs: the graphics
API and renderer as the driver reports them (and, in amber, why it is not the one asked for, if Vulkan could not
start); the clock the scene reads (the local time,
or the override) and the moment the backdrop was last handed, which Theme Settings' Colour, the XMB's start and the
music hold; **Override the time**, a month, day and time of day the clock is held at; the perf overlay's levels;
**AE**; the arguments the app was launched with; the config's path; and, on Windows, **Start with Windows**
([Installing](#installing-windows)).

## Frame pacing

The cap is on at 60 by default, and 6 at the least; the config keeps it, or 0 for none. Without one each frame waits for
the display's next refresh (swap interval 1). With a cap that divides the
refresh rate, such as 60 on a 240 Hz display, each frame waits for that many refreshes (swap interval 4), so every
frame lasts exactly as long. Any other cap is kept by a timer with vsync off: a deadline one period after the last,
slept towards and spun for its last 0.8 ms: std's sleep on Windows is a high-resolution waitable timer, which woke at
most about 0.6 ms late here whatever the system timer resolution (measured with it at 15.6 ms, 2026-10-28), so raising
the resolution (`timeBeginPeriod`) would buy nothing but more wake-ups system-wide; the spin was 1.5 ms, from 2 ms out.
The perf overlay shows the resolution and the spin. The previous version slept a period from each frame's start, then waited
for the next refresh as well, so frames came out four or five refreshes long and averaged 57 to 58.

Each frame asks for the next as it is drawn, as well as from winit's `about_to_wait`: in Windows' modal loops - a
window dragged, the tray icon's menu open, a caption button's tooltip - messages are still dispatched, so `WM_PAINT`
still comes, but the loop never comes back to `about_to_wait`, and the scene stood still until the loop ended (a
window held in its keyboard move for 3 s: a 3,002 ms frame before, 18.5 ms after).

On Windows nothing is drawn while no one can see it: while the session is locked or its display is off
(`presence.rs`, from the system's own notifications), the loop just waits; the Resources tab adds the other programs'
conditions (below). And should vsync stop waiting - with no
refresh to wait on, it returns at once, and the frames ran at 500 to 950 a second unseen - a frame is held to at
least nine tenths of the refresh period by the clock, which never waits while vsync works.

The panel's **Frame rate** (its top) sets the cap at any time, through the same pacing: **Auto**, the cap's checkbox, a
value, and under them *Suggestions*: the display's refresh rate divided by 2, 3, 4 and 8, those of 24 and up, never
the rate itself (30, 60, 80 and 120 on 240 Hz; 36, 48 and 72 on 144 Hz), each kept by the display; 30 and 60 where the
rate is not known. Auto, the default (the config's `fpsAuto`, which configs from before it take too), is the
benchmark's choice: 60, or 30 for a PC that could not draw 75 frames a second uncapped - and 30 while the software
renderer draws. A cap chosen by hand leaves Auto and is kept in the config; `--fps` overrides the config's for the run
it is given to, without changing what the config keeps.

## First start: the benchmark and the intro

`intro.rs` and `machine.rs`, the app's own. When the display drivers are new to Nagare - DXGI's adapters, their PCI
identities and driver versions, against what `machine.json` (beside the config, apart from it) last saw; so at the first
start too - the **benchmark** runs: behind the icons, every window minimized first (as Windows + M does), the scene is
drawn as it always is, uncapped and unseen, under a black screen of the console's dialogs - two lines across the screen,
and between them what is going on and a progress bar. It starts black and lets the scene through over half a second,
blurred as under the epilepsy warning (`epilepsyWarningBlurPx`), the blur going over the same half second; the scene
itself comes in slowly under it, by the XMB's own fade - the background hidden at once and shown again, as
`backgroundFadeMs` times it, but over 9 s (4.5 s when short), whatever that setting holds. As the result comes, the
black (on an ease-in, lingering then going) and the blur come back together over a second.
The scene it draws is its own: midnight on 1 January 2007, whatever the clock, with no music theme, no Season Pass and
no Theme Settings' Colour or Brightness (the music let go at once as it begins, and back afterwards as it always comes), and maru-chan sailing across in 13 s
(6.5 when short), music or not - the settings themselves untouched - until its screen is black again, when his voyage ends. The blur's radius grows and
shrinks with it, from `epilepsyWarningBlurPx` down to nothing, at full strength: only its last 3 points, below which
the blur's passes blur no less, fade in and out instead (as for the epilepsy warning's). The lines lie `benchmarkLineInset` (0.185) of the canvas's
height from its top and bottom; the warning and the intro are laid out on a 16:9 canvas fitted into the window, so that
a 4:3 screen leaves more room above and below. It stops by the wall clock, 2 s of warm-up and 8 s measured
(frames held back for a pause start the count again), and counts the frames drawn; what it measured and the cap that
chooses show for 13 s ("Reached 667 FPS, uncapped. Nagare will draw at up to 60 FPS. You can change this later in
Settings.", Nagare in bold), the bar running down the time left, then fade. `machine.json` keeps the drivers, the frames a second and the renderer. Resources'
*Benchmark (5 sec)* and *Benchmark (10 sec)* run it at any time, short or full.

Once the first start's has run, a benchmark is **short**: half as long (1 s of warm-up, 4 s measured, the result for
6.5 s), and its result says only what changed - the frame cap Auto moved to (or that it kept, or that a cap chosen by
hand stays), and the renderer it got back, if it did. **Getting back to the best renderer**: each launch starts with the
renderer the Resources tab names, and a fallback lasts only for that run; within a run, every 10 s the display drivers
are read again (DXGI, Windows), and new ones - a GPU come back or gone, a driver installed - make the window again with
the renderer asked for and run the short benchmark. A scene fallen back from a renderer that drew earlier in the run -
a lost device, say - also tries it again every 5 minutes while the settings are closed, and the wave's own fallback to
the CPU counts too. A renderer that never started in the run is not tried again until the drivers change, so that a PC
without it does not flicker every 5 minutes.

Then the epilepsy warning, if it shows before the start, and, at the first start ever (`intro`, the Scene tab's First
start; the config keeps `introSeen`), **the card**: "YOU", "WILL", "OWN" and a fourth word, a line each, set from the left on
the pale backdrop of the console's 2009 advertisements, fading in from black over 6.7 s; still for 3 s, then the last word rolls by sideways, as the XMB's categories go by - 49 words: NOTHING, A
LOICENSE, then NOTHING with A LOICENSE and A TV LOICENSE now and then - the one in the slot in full ink and the others dimmer, fading with
their distance, blurred with its speed (painted at points over a 1/30 s shutter, a pixel or so apart). It clicks once, on to A LOICENSE, holds there for a second, then spins on over 7 s, slow at first, gathering speed, and slowing to a
stop on EVERYTHING as a wheel's pointer does, catching on each word and flicking past it, the last ones slowly; then
the others fade and the card goes black. The XMB's
start then plays from its beginning, AE starting again from nothing (as it does whenever the XMB's start plays again),
and 13 s into it a notification, "You have earned a trophy.", points the new user
at the Nagare Orb (Norb) in the tray for the settings: a toast in Nagare's name (registered for the user under
`HKCU\Software\Classes\AppUserModelId\Nagare`, which the uninstaller removes), a reminder on screen until its one button,
"Legally-distinct X/O button to dismiss this message", is pressed, with the logo at 128 px beside the text, or, should the toast fail, a balloon from the tray icon. Under the intro the scene draws as it is, unseen; a `--test` run plays none of it
but with `--first-start`.

## Perf overlay

`--perf-overlay N` or the panel's buttons. The overlay draws itself last, with its own egui context and painter, so its
cost is timed apart from everything else and shown with the frame's. Where the frame is too short for it, the whole
overlay is drawn smaller, in steps down to 60 %: a step down as soon as it does not fit, a step back up only once the
larger one has had 6 % to spare for a second, so that an overlay at the edge, whose height moves a line or two, does
not flick between two sizes.

1. The frame time and rate, averaged over the last half second, the overlay's own CPU and GPU time, and the graphics
   line the panel shows (the API, the driver, and where the spline mesh is built: `spline mesh on GPU (f64)` and the like).
2. The frame's spread, the pacing, the system's timer resolution (now and its finest, by `NtQueryTimerResolution`)
   with how long the frame cap's wait spins a frame, and three tables:
   - each stage's time on the main thread (the scene's sets; listening - the music's response and the sub-frames; the
     wave's simulation and draw; the particles' simulation and draw; the panel; the overlay; the swap, mostly waiting
     for the display; and the frame cap's sleep). The stages that draw only record the GPU's work, which the next
     table times;
   - each pass's GPU time: the spline surface (under Vulkan, when the GPU builds it), the backdrop, the wave (all its
     sub-frames), the composite, the glare or the bloom, the display, AE, the particles, the panel and its blur, and
     the overlay - only the passes that ran. Each is measured between marks in the frame's commands, timer queries
     under OpenGL and timestamps under Vulkan; the GPU starts a pass once the last has written what it reads, so each
     time is that pass's own, give or take an overlap at the edges;
   - each thread's share of a core, read on a thread of its own once a second: the main thread, the driver's
     (`nvoglv64.dll` on NVIDIA), the app's helpers (the audio's analysis among them). It reads only the app's own
     threads, through handles it keeps: a snapshot of every thread on the system, as it took before, stalled the
     frame it fell on by more than half a millisecond, once a second.

   Then, as tables of a label and its value whose numbers keep their widths, the frame's size and particles alive, the
   simulations' steps that frame, auto exposure's reading and the process's CPU load; what is heard of the music,
   drawn and in a table; and what the app is doing for other programs (running, limited, paused, stopped, and why),
   the GPU's memory (all programs', and the app's own), the CPU and where the threads are placed, and the CPUs the
   main thread ran on in the last second (P or E on a hybrid CPU). From level 2 the overlay keeps one width, so that
   nothing in it moves as the numbers change.

   With the panel's **maru-time** ticked (kept in the config; on in a new one), the music is drawn too, at every level - under the
   frame time at level 1, between the readouts at 2 and 3 - as **Nagare-maru**, a little steamboat (`steamboat.rs`),
   an original cartoon. The sea is a
   calm swell at a steady pace, always the same, while no bass is heard; once the bass's pitch is, it turns at once
   into the bass's waveform as a media player's scope draws one - a crest across the drawing for every 10 Hz, as tall
   as the bass is loud from moment to moment, running fast and a little restless - and is the swell again the moment
   the bass goes. The boat stays where it floats, the waves passing under it. On each kick it nods nose up, as far as
   the kick is strong, its funnel squashing, and a note rises beside it every other kick; it trembles with the bass
   at subShake's rate, and blows steam with it, the more the louder; its face blinks, squints on a kick and sings
   with heavy bass - its mouth open once the bass's level is over a half, the wider the further over and the lower
   the bass, widest at 45 Hz and under, a third of that at 100 Hz and over (`maru::mouth`, the scene's boat's too). The moon glows with the bass and sways its head a little on the beat, left then right. Not listening - the music theme off - it sleeps at anchor, z's rising from
   its funnel. *maru-time*, in its corner, is what drawing it takes on the CPU (a few hundredths of a millisecond).
3. A graph of the last 300 frames, between the tables and the rest: each frame's interval, the CPU's work, the GPU's
   total and the overlay's own CPU.

While the overlay is off nothing of it runs: no stage is timed, no GPU query or timestamp is taken, the readouts and
the music's snapshot are not gathered, the threads' sampler stops, and the steamboat neither moves nor draws.

Measure release builds: in a debug build egui's painter checks for GL errors after its calls, and each check waits for
NVIDIA's threaded driver to catch up - spinning, so the overlay alone takes a whole core there, and its stage absorbs
the wait for the display.

## Enhancements

Everything the native app adds to the scene sits at the top of its tab under **Enhancements**, each section titled
**[Enhancement]**, above the console's own parameters under **Console-original knobs** - except on the Scene tab, whose
console state (the theme, the sequence, the music, Theme Settings, the video output) comes first. With every enhancement off the
scene is drawn as the console draws it; by default the glare's mode is the enhancement (the exponential bloom), and the
tonemapper Gran Turismo's.
The music's are on the Scene tab, the rest on the Post tab. The app's own tabs, **Panel** and **Resources**, hold
settings of the app rather than the scene.

## Music
On the Scene tab, under *Automatic Music Theme* - with a note saying when it works: which apps register a media
session (GSMTC), and what the visualizer's WASAPI loopback hears - and *Music Theme Audio Visualizer*, whose settings
stand in groups by their names' first word (audio, tempo, bass, sub, kick, motion):

- `autoMusic` (on, *While playing*, by default): plays the music scene while Windows' media sessions - the ones its
  media flyout shows controls for, from Spotify, a browser's tab, the Media Player - report one playing; *While
  controls show* while any session exists, paused too. It adds to the panel's Music setting, which still turns the
  music on by hand. `autoMusicHoldSec` keeps the music scene a moment after the media stops, so a gap between
  tracks does not drop it. The panel says what the sessions report, the app included. Windows only; `media.rs` reads
  them on a thread of its own, once a second.
- `musicColourChoice` (*Original music theme (dank purple)*; formerly `musicClockColour`, which configs still read): *Normal
  color (clock, fake clock, or Season Pass)* keeps, while the music plays, the colours the scene has without it - the clock's,
  the overridden clock's, Season Pass's or Theme Settings' Colour's - the
  backdrop's program, its day and night, and BACKGROUND.mnu's four colours - in place of the music's purple and its
  dark floor, while its motion, placement, exposure and glare stay the music's; `musicColourMix` (formerly `musicClockColourMix`; 0.5 by
  default, and at most) sets how far.

### Motion blur

`motionBlur` (on) draws the frame in sub-frames while the pokes or the music move the camera or flicker the brightness
within it - as many as bring it to `motionBlurHz` (240): four a frame at 60 fps, two at 120, one at 240 - each with the
camera and the brightness of its own moment, and averages them, as a shutter open for the whole frame would: a bop or
a sub shake blurs into motion instead of jumping between frames, and a flicker faster than the frame rate averages out
rather than showing as a slower one. The wave's sub-frames add up in a float buffer, which the composite averages and
saturates as the console's 8 bits do; the particles are each drawn at their share of the light. Frames where nothing
moves are drawn once, as before, to the byte. At 60 fps, four sub-frames cost about 0.6 ms of GPU on an RTX 2070
Super, while they are drawn. `NAGARE_SUBFRAMES=n` draws n unmoved sub-frames whatever moves: the wave comes out to the
byte as one; the particles, each a quarter of its light rounded into 8 bits, within a step or so.

### The Music Theme Audio Visualizer

[docs/music-response.md](docs/music-response.md) collects research on making the response closer to what a listener
hears - perceptual loudness, mel bands, better onsets, downbeats, latency - none of it built yet.

While the music holds the scene (by hand or by autoMusic), `audio.rs` listens to what Windows plays - WASAPI's
loopback of the default output, on a thread of its own at the lowest priority, started only then - and the scene
reacts, never as a visualizer's instant-by-instant pumping:

- `audioReactive` (on): the wave's brightness flickers - by up to `audioFlicker` (0.42, so 42%; up to 1) either way,
  about `audioFlickerRate` (2.5) times a second, partly at random and partly (`audioFollow`, 0.5) with the music's
  own swells over its average - in proportion to the *activity*: the loudness averaged over the last `audioWindow`
  (10) seconds, pauses included, between `audioQuiet` (-40 dB) and `audioLoud` (-20 dB), risen over `audioRise`
  (3 s) and fallen over `audioFall` (6 s). `audioBassBoost` (100 dB, up to 1000) makes the bass count for more in that
  loudness: the kick's band (below 100 Hz) and the bass's body (around 180 Hz) are added again at that gain - at the
  default, so far that the loudness is the bass's alone. A
  podcast, with its gaps, barely stirs it; loud, bass-heavy music does.
- `tempoBounce` (on): finds the music's beat and bounces the camera on it, as a poke does, by `tempoBounceAmount`
  (0.02), once the activity has reached `tempoActivity` (0.6) and the beat is sure enough (`tempoConfidence`, 0.2).
  - The onsets (`tempoOnsets`): *Bass and percussion* (the default) reads each 10.7 ms step's rise in four bands'
    energy - the kick's and the sub's (a low pass at 100 Hz), the bass's body (a band pass at 180 Hz), the snare's
    crack (2 kHz) and the hats' (a high pass at 7 kHz) - the first two weighted by `tempoBassWeight` (1.5) and 0.6 of
    it, the snare by 1 and the hats by 0.5; a rise under 0.65 dB, a held note's ripple, is none. *Whole band* is the
    low band's and the whole band's, as before.
  - The tempo: over the last 8 s, each onset taken above the average of the quarter second around it, their
    autocorrelation, averaged over the last evaluations and summed over each period's multiples (a beat repeats at two,
    three and four periods; a half beat does not), weighted towards 120 a minute, gives the period (70 to 180 a
    minute), changed only when another is found three evaluations running. The phase that lines a train of that
    period up with the most onsets gives where the beats fall, and the beats already followed are only nudged a third
    of the way towards it each quarter second, as a phase-locked loop follows, so each bounce starts just before its
    beat and does not jitter.
  - The gate (`tempoGate`) holds the bounce back when the song goes quiet, so that a beat still followed does not
    bounce on into a breakdown; each bounce is as large as the gate is open, which opens in 0.15 s and shuts over
    0.5 s. *Loudness now*: the last moment's loudness against the window's, which starts shutting `tempoGateDrop`
    (10 dB) under it and is shut 6 dB further down, or below `audioQuiet`. *Bass and drums now* (the default): the
    same with the hits - the bands' onsets, bass-weighted, over the last half second against the window's - so that
    it shuts when the drums stop however loud a pad plays on. *Beats still heard*: by how strongly the last two
    beats were heard where they fell, against the onsets' average. *Off*.

  On real playback of a drum loop, the bands found its 120 a minute with a confidence of 0.75, where the whole band
  found 0.28. Halving and doubling (a song at 87 or 174 a minute) stay a matter of taste, which the lean towards 120
  settles.

The bass has effects of its own (Scene tab, *Music Theme Audio Visualizer*, its bass, sub and kick groups). Its pitch is read every 10.7 ms: the output low
passed at 180 Hz (twice over), taken down to about 1.5 kHz, and YIN's pitch detector run over the last 85 ms between
`bassLow` (30 Hz) and `bassHigh` (90 Hz); its level is that low passed signal's, between `bassQuiet` (-40 dB) and
`bassLoud` (0 dB), followed up in 30 ms and down in 150.

- `bassFlicker` (on, with audioReactive): the brightness flickers at the bass's pitch times `bassRate` (0.25x, 0.5x -
  the default - or 1x), by up to `bassFlickerDepth` (3%) as the bass is loud - held under half of bassRate, 0.5 at 1x, 0.25 at
  0.5x, 0.125 at 0.25x, so that the slower flickers, which a display shows as they are, flash less: a 42 Hz sub at 0.5x flickers at
  21 Hz, a 100 Hz kick's body at 50. A display shows no more than half its frame rate, so `bassFold` (on) halves the
  rate an octave at a time until it is under 0.45 of the frame rate (27 Hz at 60 fps), holding the octave until the
  pitch leaves it by a margin; lower bass still flickers slower. The shake, below, is folded under the rate the frames
  are drawn at instead, their motion blur's sub-frames counted, so that at 60 fps it runs as fast as at 240. Off, a faster rate shows as whatever the frames catch
  of it - an alias, slower, rising and falling with the pitch.
- `subShake` (on): the same wobble moves the camera forwards and back, by up to `subShakeAmount` (0.042): the wave
  shaken by the subs, not bounced. Its rate and depth are the bass's: the pitch must lie between bassLow and
  bassHigh, the level between bassQuiet and bassLoud sets how far it shakes, bassRate and bassFold its rate, and the
  warm-up gates it; bassFlickerDepth does not touch it.
- `kickBounce` (on): a bop on each kick as it is heard, by `kickBounceAmount` (0.02), half to one and a half times
  as the kick hits softer or harder. A kick is the low bands' (below 100 Hz, and around 180 Hz) rise past
  `kickThreshold` (1, about 4 dB in 10 ms) while they hold a quarter of the step's energy - a snare's noise rises
  there too, but carries little - and no sooner than 120 ms after the last. It comes as the kick is heard, a frame
  or two after it; the beat's bounce comes on the beat found.

All three wait for music: once a sure beat (`tempoConfidence`) has been heard for `bassWarmDelay` (3 s), they open
over `bassWarmUp` (8 s), and from then on answer the bass at once, through pauses and between songs, until
`bassWarmHold` (3 minutes) has passed without music; then the next music warms them up again. The beat is looked for
whenever a bass effect is on, even with tempoBounce off.

The panel (under the settings) and the perf overlay say what is heard: the loudness now and over the window, the
activity, the brightness's gain, the bass's pitch, level and the rate it is shown at, the kicks, the hits, the tempo, how sure and how strongly its beats land, the gate, and the
analysis' share of a core - about 0.15% on a desktop CPU. Nothing of it runs in `--test`.

The Scene tab's `backgroundFadeMs`, the fade when the XMB's background is hidden or shown (and a stop's or a return's
*Background fade*), is 10 s by default here, up to 20 s; the page keeps the console's callers' 200 ms.

## Tonemapper

`tonemap` swaps the console's curve, x (1 + x / W²) / (1 + x) per channel, for another: **ACES** (Stephen Hill's fit
of the RRT and ODT), **AgX** (Troy Sobotka's, in Benjamin Wrensch's minimal fit, with its Golden and Punchy looks),
**Gran Turismo** (Hajime Uchimura's curve, with its six knobs: max brightness, contrast, linear start and length,
black tightness, pedestal), **Hable** (Uncharted 2's, with its white point) and **Reinhard** (extended, on luminance,
so hues hold). The console's light is a display's already - its curve writes it straight to the screen - so each
takes it as a 2.2 display's into linear light and goes back after. `tmMatchMidGrey` (on) works out a gain that
brings mid-grey through the curve back to mid-grey, so the mid-tones stay the console's and only the shadows and the
highlights change; off, most curves come out darker. `tmGain` and `tmSaturation` adjust any of them.

Under them the curve comes last: the composite writes the exposed light, in half floats, instead of the console's 8-bit
screen; the glare or the bloom is drawn out of that light; and the display pass puts the screen and the glare through
the curve together, then the highlight desaturation and the dither. Auto exposure measures the light as the curve shows
it. The glare's mask still reads the console's curve, so what glows is what glows on the console. Under Console the
passes are the console's, unchanged.

## Auto exposure

**AE** (on by default) raises the exposure while the screen's peak stays below a target, by at most `aeMaxPush` (0.99 by default),
rising over `aePushTime` (1 s) and falling back over `aePullTime` (2 s); and once the peak reaches `aePullAbove` (0.99, well
past the target, so that it does not hunt) it lowers it, by at most `aeMaxPull` (0.3), over `aePullTime`. Between the
target and `aePullAbove` a boost holds, and a cut stays until the scene darkens below the target. At the start it
waits: off for `aeStartDelaySec` (7 s), so that the start shows the scene at its own brightness, then its speed builds
up over `aeBuildUpSec` (14 s), its times divided by how far the build-up has come, so that it is at full speed 21 s
after the start; the panel's values are left as they are. It aims towards `aeHighlightTarget` (0.95) or
`aeLuminanceTarget` (0.9), whichever `aeMetering` measures (luminance, by default). The exposure setting is left as the scene's sets write it;
the composite draws with the sum. Its knobs and reading are on the Post tab, above the tone curve. How the peak is
measured:

- `aeSource`: *Final frame* (the default) meters what goes to the screen - the glare or the bloom and the tone
  curve with it - before the particles: under OpenGL a copy of the window's frame, under Vulkan the canvas itself.
  *Before glare* meters the composite, as AE did before; the glare it does not see can carry the highlights past
  the target.

- `aeMetering`: a pixel's brightness is its brightest channel (no channel clips) or its luminance (Rec. 709; the default).
- `aeZoneSize`: the screen is cut into square cells of 32, 64 or 128 pixels (the default), each taking its brightest pixel.
- `aePercentile`: where the peak is read among the cells, 0.97 by default (the brightest 3% may pass the target);
  1 takes the brightest cell, so that no highlight, however small, is pushed past the target.

It runs entirely on the GPU: reduction passes, then a one-texel pass that takes the quantile and moves the boost on,
ping-ponged in a 1 × 1 texture the composite reads. That pass finds the quantile by halving the 256 levels eight times,
counting the cells at or above each, rather than with a histogram: an array of 256 counts in a program costs the
drivers scratch memory for every thread the GPU can hold - 40 MB under Vulkan, 10 MB under OpenGL, with AE off too -
for the same count. Reading anything back in the frame would wait on the driver's
frame backlog, a whole refresh on NVIDIA. The readout the panel and the overlay show is read back at most twice a
second, and only while they show it - under OpenGL into a pixel buffer behind a fence, taken a later frame once the
GPU has passed it, so that nothing waits.

## Glare mode

The Post tab's **glareMode** picks the glare. *Exponential bloom*, the default, is the native app's alternative to
*Console*, the console's chain as ported: the console's glare source (the screen above GLARE THRESH, its mask times 8)
read into half the screen's size, halved octave by octave with a 13-tap filter, and summed back up through a tent
filter with weights that change geometrically per octave - a sum of blurs of doubling width, whose profile falls off
exponentially as a lens's scatter does, at the screen's resolution rather than the console's 128 × 64, so small
highlights glow smoothly instead of in soft blocks. Its light adds up to the same GLARE LEVEL^2 as the console's, so
the theme's glare settings still drive it, and:

- `bloomStrength`: a multiplier over GLARE LEVEL.
- `bloomFalloff`: each octave's weight against the next finer one, 1.2 by default, which puts the light about as far
  out as the console's levels; lower keeps it near the highlight.
- `bloomOctaves`: how many octaves, so how far it reaches: 8 to 64, 8 by default. Past one pixel the octaves stay at
  one pixel, each adding the screen's average again - a haze over everything.
- `bloomTint`: how much GAUSSIAN RAD R, G and B colour it, each channel's falloff scaled by its radius against
  green's, as the console's per-channel blurs do.

It costs about 0.25 ms of GPU time more than the console's at 1280 × 720 on an RTX 2070 SUPER.

**Glare radius (Console mode)**, shown while `glareMode` is Console (`glareRadiusScaleOn`, off by default; `glareRadiusScale`, 2×, from 0 to 10×) multiplies GAUSSIAN
RAD R, G and B as the passes read them, leaving the settings the scene writes alone. The console's Gaussians widen, with
up to 31 taps a side instead of 7 where they need them. The exponential bloom reads only the radii's ratios, which a
common multiplier leaves alone, so it does not change it.

## Sensor noise and film grain

`noiseEnabled` (on by default) adds noise to the composite's light, before the exposure, the tone curve and the glare,
fresh every frame, of one of two kinds (`noiseMode`).

**AlexaGrain**, the default (`grainAmount` 0.13, `grainSize` 1.5), is a naive take on an ARRI ALEXA's grain as Riley Gray's
[AlexaGrain](https://www.nukepedia.com/tools/gizmos/draw/alexagrain/) gizmo for Nuke draws it: four noises at the
sensor's own resolution - a grey one 2 sensor pixels across and one in each channel, red 6.5, green 4 and blue 8 - each
a sum of octaves of gradient noise (fBm), weighted 0.035, 0.03, 0.01 and 0.022, the grey one added to each channel,
times 1.6, 2 and 3, so that blue grains most; the sensor resized to the screen (`grainSize` 1 is the gizmo's 3.4k
ALEXA SXT sample across the screen's width); times the camera's response to the light, each channel's own curve (the
gizmo's Film_Response), so that there is more grain in the light than in the dark; added to linear light. `grainIso`
reads the response at the ISO's exposure and brings it back, as the gizmo and the camera do, so that a higher ISO grains
more; `grainAmount` is the gizmo's Overall Amount. The noises are baked once into a seamless 1024 × 1024 tile (again
when the size changes), each octave's lattice turned by an angle of its own, so that no direction or period lines up
across octaves or with the screen's pixels (the moiré a lattice shared by every octave beats into); each frame reads the
tile twice, from places of its own, turned one of eight ways, the sum scaled back by 1/√2, so that the grain is new
every frame for a texel a pixel. Left out: the gizmo's per-channel sharpen and soften (0.25 / 0.25 / 0.1 and off),
its resize in LogC, and its blacks and whites keys, which do nothing at their defaults.

**Sensor** (`noiseRead` 0.0005 by default), the app's own, adds camera sensor noise
curve and the glare, fresh every frame. It is added in linear light, where a sensor counts photons: the console's light
is a display's, so it is taken through sRGB's curve to linear, given Gaussian noise with a standard deviation of
sqrt(`noiseRead`² + `noiseShot`² × light) - a read-noise floor and shot noise growing with the light's square root -
and taken back. Back on the screen a step of linear light is worth far more in the shadows than in the highlights, so
the grain shows in the dark and fades in the bright, as a photo's does; sRGB's straight foot keeps true black from
sparkling. The exposure and auto exposure's boost then amplify it as a camera's gain does.
`noiseChroma` mixes grey grain with per-channel grain, `noiseSize` sets the grain in pixels.

## Particle limit

`particleLimit` (2049; Particles tab) is the size of the pool the emitter takes particles from - 2049, the XMB's own, as
read from a savestate, up to 65536. The console runs it nearly full, but at rest the emitter keeps only about 2,600
alive whatever the pool, so a larger one fills only as `emitPerFrame`, `emitProb` or a slower `agingSpeed` ask: with
`emitPerFrame` 40 and a limit of 16000, about 6,700. Grown, the pool's new slots come free; shrunk, it comes up empty and
fills again. The simulation's cost grows with it, on one core.

## Maru-chan

**maruChan** (on by default; the Maru-time, etc. tab) puts Nagare-maru, the perf overlay's steamboat, into the scene as a
little 3D boat (`maru.rs`) that now and then sails the wave across the screen, from off its left edge to well off its right
(six tenths of the screen's half-width past it, so that it and its smoke are gone before it is), at the speed that
crosses the screen in `maruCrossSec` (30 s); `maruSizePx` (95, up to 420) pixels long wherever it floats. It rides the
highest of the wave's lines where it is across the screen, the wave's top edge; `maruRideDepth` (0) lowers it that
share of the way down to the lowest, 1 its bottom edge, its buoyancy and tilt following the same line. Its first voyage comes `maruFirstMin`
(0.5) minutes after it is switched on, or after the app starts with it on, then one every `maruEveryMin` (0.5) minutes
from the end of the last. Under the music its first comes `maruMusicFirstMin` (0.5) minutes after the music begins,
then one every `maruMusicEveryMin` (0.5) minutes, and when the music ends the next comes `maruEveryMin` after it; the
music's coming and going changes only a wait, never a voyage under way; with `maruMusicOnly` (on) a voyage that comes due waits for the music theme. **Sail now** sends
it at once.

- The model is made at start in the 2D boat's own proportions - its outlines drawn out into slabs with rounded rims:
  the hull with its stripe and porthole, the cabin, the funnel and its band, the mast and pennant - and always faces
  the eye, so that the scene stays 2.5D. Its face is the 2D one, painted on the cabin's front.
- Its body is frosted glass, as the XMB's icons are: what lies behind it - the wave and the particles - seen through it
  bent by its surface, a little blurred, its colours split at the edges, and tinted by the glass's own colour, with a
  sharp highlight, a soft one from the sky and a rim of light. Each renderer copies the frame as it stands for it.
- Where it floats is read off the wave each frame, from the vertices the particles are emitted from: where the wave's
  lines cross its place across the screen, its height is the highest of them and its distance the middle of their
  band, both eased, with a slow bob and roll.
- Of the 2D boat it keeps the face - blinking, squinting at a kick, singing with heavy bass, its mouth the wider the
  lower the bass, as the 2D boat's; the bass moves it up and
  down at subShake's rate. Its smoke is soft sprites, broken by noise, rising, swelling and drifting back, faster and
  more the louder the bass, and a burst when heavy bass comes in all at once (as the 2D boat's does); `maruSteam` (7, up to 69)
  scales it.
- `maruFlat` (on) draws it flat, as an icon: square to the screen, every part projected at the boat's own distance, so
  that the perspective places and sizes it but does not distort it. `maruSaturation` (0) is how much colour it has - none,
  white glass as the XMB's icons are, to 1, the 2D boat's. `maruBuoyancy` (1) floats it on a spring that follows the wave's
  every rise and fall, overshoots, leans and hops at a kick. `maruTilt` (1) tilts it with the slope of the surface under it - read a little
  behind and ahead - on an underdamped spring, `maruTiltBounce` (1) its bounciness, so that a small change of the
  slope rocks it about. `maruBassBounce` (1) scales how far the bass moves it up and down. `maruSmoke` picks *Puffs* or *Swirling* (the default): many finer wisps
  carried by curl noise - the curl of a field of waves drifting in time, which has no divergence, so the smoke eddies
  without bunching - at real-time cost. Its triangles are sorted along its own front-to-back axis, so that what is
  laid on others (the stripe, the face) never flickers through them.
- `maruSupersample` (off; 2x, 3x, 4x, the default) draws the boat - not its smoke - into a tile of its own over its box on the screen,
  that many times the screen's pixels, kept and grown in steps of 128 rather than made each frame, and averages each
  block back onto the screen: its edges, the stripe and the face's strokes as smooth as supersampling makes them, where
  FXAA only softens them. The glass still reads the scene where the boat is. About 0.05 ms of GPU more at 4x and 140
  px.
- `maruMotionBlur` (on) blurs its motion as an open shutter would: from the very terms that move it - the bob, the
  bass's bounce at its phase, the buoyancy spring's speed, the crossing - it works out where it was at moments back
  through the frame, as many as its travel on the screen needs (none when it hardly moves, up to 16), draws each whole
  into a scratch tile and adds it into the boat's tile at its share. Drawing each place whole first keeps what is laid on
  others (the face, the stripe) on them, where adding the places' triangles straight in would show through.
- `maruSceneLight` (on) lights it by the scene around it, so that it sits in it rather than glowing at its own
  brightness: each frame a 16 x 9 map of the frame's light is taken from the copy the glass reads (each texel the mean
  of its part of the frame, 64 taps) and eased over 0.3 s, so that a flash or a passing particle does not flicker it - in half floats, and taking the
  frame in at most 60 times a second, the time between carried over, so that it eases alike at any frame rate (in 8 bits
  at hundreds of frames a second, an uncapped benchmark's, each step rounded away and the map froze);
  the boat's own light - its glass's body, its highlights, the paint and the smoke - is then scaled by how bright the
  frame is around it and on the whole, against a frame of middling brightness, and tinted its colour as far as
  `maruSceneTint` (0.5). It is dark while the scene is, as at the start, and follows the exposure, AE's too, since the
  map is taken from the finished frame. What the glass shows of the scene behind it is the scene's already.
- It is worked out on the CPU - placed, its faces turned away from the eye left out, the rest sorted far to near (no
  depth buffer is needed for a small closed model) - into one list of triangles, which both renderers draw with the
  same two shaders, after the particles, so that the glass shows them. About 0.3 ms of CPU and a few microseconds of
  GPU a frame while it is out, nothing otherwise; the perf overlay shows it as *Maru-chan*.

## Disco ball

**discoBall** (on; the Maru-time, etc. tab) brings a mirror ball down once the music theme has played heavy bass for
`discoAfterMin` minutes (0.5): bass louder than `discoBassLevel` (0.42) while the visualizer listens adds up, anything
else drains it at half the pace - or, with `discoWithMusic` (off), as soon as the music theme starts. Once down it
stays down, whatever the bass, until the music ends; should the music come back while it is still going up, it comes
straight back down - the bass is asked for again only once it is all the way up. It is lowered on its cable over `discoDescendSec` (42 s), at an even pace, as a
motor winds it, at `discoX` of the screen's width from the left (0.67: about a third of the width in from the right),
swings on it - a pendulum of about three seconds, nudged by each kick - spins at `discoSpin` turns a second (0.015),
and hangs there until the music ends, when it is drawn back up over `discoRiseSec` (42 s), as evenly. Drop now on the tab lowers it at once, until the music next ends. It needs the music to be heard: one of
the visualizer's effects on.

- The ball (`discoSizePx`, 84 px across, up to 256; supersampled at `discoBallSupersample`, 4x by default, as
  maru-chan is: its shading worked out at n x n points a pixel and averaged, its edges softened over a sample) is no mesh: `disco.rs` draws it per pixel over its box, working out the
  sphere under each pixel - flat (`discoFlat`, on), seen square on with its rows of tiles level as an icon is, or seen
  from below as it hangs above the eye - turned by its spin, and finding the tile the pixel falls on. The tiles are laid
  in 50 rows (`discoTileSize` 0.32; at 1 they are about three times the size, 16 rows, and the spots grow and shrink
  with them), fewer round the rows nearer the poles, as real ones are; each is flat, set a hair askew as glued glass is
  and faintly smudged, and shows one reflection: the frame as a fish-eye sees it, mixed with the frame turned upside
  down, so that a dark lower screen does not darken the ball's lower half, and in the tiles that look down a lit dance
  floor (`discoFloor`, 1) that the ball alone reflects. Between them, sized as a share of the tiles - so that a small
  ball has fine gaps - is the dark adhesive they are set in, each tile's edge a bevel catching the light. The glass -
  what it reflects and the lights it catches - is taken to `discoGlassGamma`'s power and times
  `discoGlassBrightness` (1.3 and 3, up to 4), over the adhesive, and `discoSheen` (1, up to 10) adds a soft light at grazing angles round
  its edge, as Blender's Principled BSDF's Sheen does, lit the more the key light lies behind the edge.
- It hangs from a cable of glossy pearl plastic, round across: dim where it faces the eye, bright at its edges by its
  fresnel, with a faint rainbow in the rim as the XMB's glass has, and a long highlight down it on the key light's side;
  its cap, a little dome of the same plastic, lies behind the ball, half of it showing.
- **What lights it** is `discoLight`:
  - **Studio lights & the sun** (the default): the studio's two lights below and the sun besides, all three lighting
    the ball and sending out its spots and beams.
  - **The sun**: the backdrop's sun. `back_colours0` sends a small glow round an ellipse once a day, at
    angle (`_DayTime` - 600) π / 1200: up the left in the morning, over the top at noon, down the right in the evening,
    below the screen at night. The ball is lit by a far light behind the scene, through that place on the screen. The
    clock is the backdrop's, so the overridden clock and Season Pass move it too; the music's backdrop draws no sun,
    but it is still there.
  - **The particles' light**: the spot light the sparkles are lit by (`spotPosX`, `Y`, `Z`), where it stands in the
    world, the ball set in the same world the camera sees.
  - **Studio lights**: a warm white key from above and one of the particles' iridescent colours from the right.

  With a sure beat (`discoBeatLights`, 2, up to 4), the lights flash on each beat where they stand - none of them
  moves - the coloured one takes the palette's next colour, and the floor changes. Under the sun alone or the
  particles' light, the coloured light only tints the ball, faintly, and sends nothing out.
- Its light is worked out on the CPU from the very tiles the shader draws, their tilts too: through a hazy room
  (`discoBeams`, 0.15), the light's beam onto the ball and the beams its tiles send back out, drifting fog in them with a
  faint grain from frame to frame, as motes catch the light - down to where one beam in seven meets the wave, and out
  into the air from one reflection in thirty that misses it; and over the ball the glints (`discoGlare`) - a star of eight rays, the long four with a rainbow's fringe -
  on the seven tiles that flash brightest, twinkling, and a soft glow that swells with them. Each light's glints are
  its own share of discoGlare: `discoGlareKey` (the studio's key, or the particles' light; 1), `discoGlareColour`
  (the coloured light; 1) and `discoGlareSun` (0.15, a far light).
- **The spots** (`discoSpots`, 0.2 of 0 to 1, a quarter of it taken as their brightness) are light on the wave, as a game lights a scene with a spot light's pattern. They
  come from a finer tiling of the ball, 60 rows at a `discoTileSize` of 1, about 4,600 tiles as a real ball has, each a
  hair askew; smaller tiles make it finer - 188 rows, about 45,000 tiles, at the default 0.32, and up to 240 rows at
  0.25 - as many spots as tiles, each smaller. The ball is
  set in the world the wave and the particles are seen in (the eye at (0, 0, 2) looking down -z, 53° up and down), 0.75
  from the eye, the wave some ten units further. Each lit tile's reflection is a beam, the tile's square in
  cross-section, spread by the light's own size; each frame the beams are drawn on the GPU, as a little rounded square
  each, into the pattern the ball projects (1024 × 1024, half floats) - every tile an instance, laid once for its
  tiling with its tint under each light, which a vertex shader turns with the ball, reflects each light off and sees
  from the ball, drawing nowhere a tile the light does not reach or whose beam misses - through a frustum from the ball round the wave - its
  surface read from every eighth of its vertices, 16 by 16 quads in the world. Then the wave's mesh is drawn again over
  the frame, each pixel's place in the world worked back from the clip space it comes in, lit by that pattern and
  falling off with the distance's square (against the distance to the wave's middle). The wave is a thin, misty
  sheet: it scatters the light back from either side as its face turns to it, lets it through towards the eye, shows
  more of it seen edge on, where its mist lies deeper along the eye's line, and carries it a little way on inside, a
  soft glow round each square (the pattern read along a golden-angle spiral). So a spot lies on the wave exactly as
  its surface takes it, on every fold the beam passes through, stretched where it glances and squeezed where the wave
  turns away. The CPU follows only a sample of the tiles, as large whatever their size, for the beams through the
  haze and the spots in the air.
- A reflection also lands, small, on a plane at the wave's middle depth, as a sprite seen only on maru-chan (her box
  on the screen) and, as far as `discoSpotsAir` lets it (0, none, by default), on the backdrop. As the ball spins, the
  spots sweep across the wave. The ball's light costs about 0.7 ms of the CPU a frame and 0.35 ms of the GPU at
  720p (the defaults, three lights), only while it is out.
- The beams and spots go under the ball, the glints over.
- The perf overlay's steamboat has one too, while the scene's is out: hanging from the top of its sky by the moon,
  spinning, sparkling, with spots about it. The overlay shows the scene's as the *Disco ball* pass.

## Season Pass

**seasonPass** (the Backdrop tab; *In order* by default) is the app's own tour of the months' backdrops, in place of the
clock's month (`scene::SeasonPass`): each month's colours for `seasonPassHoldMin` (4.2, at least 3) minutes, as on its 1st at the clock's own hour - the backdrop
brings the next month up the screen as a month goes by, so that only at its start does a month show its own colours alone -
then the next fades in over `seasonPassFadeSec` (42, at least 30) seconds - *In order*, the calendar's, or *Shuffled* - among the months
its toggles pick (`seasonPassMonths`, as bits, all by default). It begins on `seasonPassStart`'s month - *The clock's
month* by default, or any month chosen, in order or shuffled, a change beginning the tour again there - or the first
picked after it. The fade is the backdrop's own ease from one moment to the next, handed the new month with that length, the
clock's ticks left alone until it is done. It runs wherever the clock's colours show - the scene at rest, or the music
keeping them (`musicColourChoice`'s Normal color) - while the XMB's start and a content's boot keep their own moments, and Theme
Settings' Colour, a month held, wins. Its section shows a swatch of each month, in two rows of six - its day's texture over its night's,
evaluated from the fits - each a toggle, the one showing glowing white as it breathes (a second up, a second down) with a play mark on it, and a line saying what the tour is doing: the month
shown and the next (chosen ahead, shuffled too) with the time left, the fade under way, or what holds the colours instead.

## The start

**startTransition** (the Scene tab's Start, the first of its enhancements) chooses how the scene comes in as Nagare starts. *XMB start* (the default)
is the console's: the wave and the sparkles coming in under the light of 10:00, before the background the user chose
takes over, as it did once a user was loaded. *Background fade* shows the scene as the user chose it at once, faded in
from black as the background is shown again - the XMB's own transition - over `backgroundFadeMs`. It is read from the
config (or `--set`) before the first frame, and it applies wherever the start plays again: after the epilepsy
warning, the benchmark or the card.

## Epilepsy warning

**epilepsyWarning** (the Scene tab) shows the photosensitive epilepsy warning as the console shows it as it starts - by
default at Nagare's first start only (the config keeps `epilepsyWarningSeen`; `epilepsyWarningEvery` shows it at every
start, and Show it now at once; a `--test` run counts as a later start):

- *Before the start* (the default): on black, before anything else is seen, 0.3 s in; as it begins to fade out, the
  XMB's start plays from its beginning - the sequence let go for a frame and started again, so that the wave's lines,
  the particles and the backdrop, eased in from black, each begin it as at the app's start.
- *After the XMB's start*: `epilepsyWarningAfterSec` (5 s) into it, about half a second after the particles come in,
  over the scene, which is blurred behind it (`epilepsyWarningBlurPx`, the panel's blur laid over the whole screen, its radius growing and shrinking with the fade),
  the blur fading in and out with the text.

It fades in and out over `epilepsyWarningFadeSec` (0.42 s) and stays `epilepsyWarningHoldSec` (4 s) between. The text is
drawn by the panel's egui pass, laid out as the console lays it - its lines set from the left in a block wrapped to
`epilepsyWarningWidth` (0.66) of the 16:9 canvas, the block centred at `epilepsyWarningX` and `Y` (its middle,
0.5 and 0.5), its letters sized to the screen's height (`epilepsyWarningSize`) - in egui's own font
(`epilepsyWarningFont`, Built-in by default) or one of Windows' nearest the console's (Arial, Segoe UI or Bahnschrift,
read from the fonts folder), white and a little see-through, so that the scene's colour comes through it. Under it a
drop shadow: `epilepsyWarningShadow` its opacity, `...ShadowOffsetX` and `Y` (0 and 1) and `...ShadowRadius` (3.5) in
pixels at 1080 lines, `...ShadowRed`, `Green` and `Blue` its colour. Two looks after the console's lettering:
`epilepsyWarningTextBlur` softens the letters by a fraction of a pixel, and `epilepsyWarningGloss` shades each line
from the top down, lit at the top, darker and a touch cooler at the foot. Blurs are the text painted again at points
over a disc, each at the opacity that adds up to the whole; the gloss is the letters' mesh with its colours shaded.
Defaults: every start, before the start, 7 s after it otherwise, the blur behind it 7 px, the gloss 0.13.

## Anti-aliasing

`antiAlias` (*FXAA, quality* by default; Post tab) smooths the finished frame - the scene, the particles and maru-chan,
after the sharpening, so that it is not sharpened back - with FXAA (`antialias.rs`), after Timothy Lottes' algorithm, re-authored: one pass that finds where
the brightness steps, which way the edge runs, follows it to its ends and blends across it, and blends a lone pixel
into its neighbourhood. *FXAA, fast* searches 5 steps along an edge, *FXAA, quality* 12, for long shallow ones; about
a quarter of a millisecond at 1080p. `supersample` (Vulkan and the software renderer) draws the scene into a canvas
1.5 or 2 times the window's size, every pass with it, and filters it down onto the screen - 2.25 or 4 times the
pixels; OpenGL draws straight into the window and has FXAA alone. The perf overlay shows FXAA as a pass of its own.

## Sharpening

**sharpen** (on by default; Post tab) sharpens the finished frame - the scene, the particles and maru-chan, not the
panel or the overlay, and before FXAA - with Contrast Adaptive Sharpening, AMD FidelityFX's CAS (MIT), re-authored in GLSL (`sharpen.rs`): one pass
reading each pixel and its eight neighbours, which sharpens by a weight that falls where the neighbourhood's contrast
is already high, lifting soft detail without ringing round the wave's bright edges as an unsharp mask would.
`sharpenAmount` (1) is CAS's sharpness, 0 the gentlest and 1 the most; `sharpenStrength` (0.5) scales what it adds,
past 1 further than CAS goes. It sharpens the grain too. Under Vulkan it takes the place of the canvas's copy onto the
screen, so it costs only its own reads (about 0.03 ms of GPU at 1600 x 900 on an RTX 2070 SUPER); under OpenGL, which
draws straight into the window, the frame is copied into a texture first (about 0.15 ms there). The perf overlay
shows it as its own pass and stage, *Sharpen*.

## Highlight desaturation (deprecated)

It sux, not recommended.

`desatEnabled` (off by default): where a pixel's brightest channel passes `desatStart` (1, where the screen
clips), its colour gives way over `desatRange` to a grey - of its luminance at `desatWhiten` 0, of its brightest
channel at 1, which burns it to white - instead of each channel clipping on its own, which shifts a bright colour's
hue and flattens it. Below `desatStart` nothing changes, and the console's tone curve is left as it is. It is applied
where the composite writes the screen and where the glare is added to it.

## The settings' look

The panel is laid out in a grid: the names on the left, the controls beside them, a padlock where the scene's sets
write the value, and ↺ to reset it (its tooltip says to what). Two choices are radio buttons, more are a list, and each
section folds. The **Panel** tab holds its look:

- `blurEnabled` (on): the settings lie over a blur of what is behind them, under a dark tint. Over the scene's own
  window the panel's window shows the scene blurred; the settings window (the wallpaper's) shows by
  `windowBackdrop` either **the scene**, blurred, cut to where the window lies over it on the screen - on any
  system - or Windows 11's own **Acrylic** (whatever is behind, other windows too), **Mica** (tinted by the
  desktop's wallpaper) or **Mica Alt** (the tabbed windows' Mica, more strongly tinted). Windows shows those only while
  the window is active, and a plain colour otherwise; **Acrylic (always)** is the older acrylic, user32's accent policy
  (Windows 10 and later), which stays while the window is inactive.
- `backdropTint`, the tint's darkness, applies to every backdrop. The scene's own - `sceneBlurRadius` (2), its reach in
  points, `sceneBlurSaturation`, its colour, and `sceneBlurGrain`, a fine grain that keeps its gradients from banding
  - show only with **the scene**.
- `windowTheme` (Windows' materials): their shade and the frame's, **Dark** or **Light** (the panel stays dark, so
  light text over a light material reads less well). `acrylicOpacity` (Acrylic (always)): how much of that shade
  tints its blur. `windowCorners`: the settings window's corners - the system's, round, slightly round or square (the default).
- `uiScale`: the settings' size, over the display's scale, 0.5 to 2. The settings' text is whiter than egui's dark
  theme, with a soft drop shadow (the perf overlay, a context of its own, has none).

Its buttons are square, as the PSP's are, and whatever is chosen - a tab, a selected button, selected text - is lit
as the PSP lights its choice: a pale bar, lighter at its top, with a fine light edge and a brighter line along its top.
egui fills a chosen thing with an odd grey no other shape has, and the pass that adds the text's shadows paints the
highlight over each rectangle of it. Hovered, a button gets a fine light edge; pressed, it is paler. The tabs are set
in Segoe UI Semibold (egui's own font where Windows' is missing), each in a faint square frame.

The blur (`blur.rs`) is the dual filter: once the scene is drawn it is copied at half size and halved a few times,
five taps a pass, then doubled back, eight taps a pass - so a wide blur costs a few small passes - and egui is laid
out first, so that the blur is drawn where the panel's window lies, under its widgets, before egui paints. Nothing is
captured while no panel shows it, and its copies are freed. For Windows' materials the settings window is transparent:
its surface's alpha reaches the compositor, which draws the material where the panel leaves it clear.

## Giving way to other programs

The **Resources** tab's *Reading the room* says what the app does while other programs need the PC, as a wallpaper engine does. Each
condition has an action - **Keep running**, **Limit frame rate** (to `limitFps`, 15, with buttons for 15, 30 and 60, under the panel's own cap), **Pause**
(nothing drawn, the last frame held) or **Stop** - and of several conditions at once, the strongest action wins:

| Condition                | When                                                                       | Default            |
| ------------------------ | -------------------------------------------------------------------------- | ------------------ |
| `whenOtherFocused`       | another program has the keyboard (the desktop and the taskbar aside)        | Keep running       |
| `whenOtherMaximized`     | a maximized window covers the scene's monitor                              | Limit frame rate   |
| `whenOtherFullscreen`    | a window fills the scene's monitor, or a program has the screen to itself   | Stop               |
| `whenOnBattery`          | the PC runs on its battery                                                 | Limit frame rate   |
| `whenGpuMemoryLow`       | the GPU's dedicated memory, all programs', is fuller than `gpuMemoryThreshold` (67%) | Stop    |

The **process watches** - three sections under it, *Process watch (limitFps)*, *(stopTransition)* and
*(stopTransitionUrgent)* - each list programs by their file's name (`crysis.exe, rpcs3`; a name without an extension
is taken as .exe), and while one runs, with or without a window, its watch's action holds: the frame rate held to
`limitFps`, or a stop through that transition. They replace `whenProcessRunning`'s one list, which a config from before
them moves to the watch of the action it had (a limit to the first, a pause or a stop to the urgent one). The config
keeps them as `processesLimit`, `processesStop` and `processesUrgent`. While the settings are open a stop is held to a
limit, so that they can be used; a watch whose program runs says so under its list, and the scene stops once they close.

The window's conditions count only while the scene is the wallpaper, which they hide. **Stop** plays
`stopTransition` - a **background fade** over `backgroundFadeMs` (the default); the XMB's **game launch**, then black as
the game takes the screen; or black **at once** - or, for the urgent conditions (`whenGpuMemoryLow`,
the urgent process watch), `stopTransitionUrgent` (at once by default), which also takes over a calmer stop already
under way - then drops the OpenGL context or the Vulkan device, and with it every
texture, buffer and program.
`stopHideWindow` decides what is left behind the icons: **Black**, the window, which keeps the few frames' worth of
buffers the driver holds for it, or **Hide**, which shrinks and hides it too, so those are freed, and the desktop's
own wallpaper shows. In a 2560 × 1440 window under OpenGL the app's GPU memory goes from 146 MB drawing to 64 MB
stopped black; under Vulkan, which drops its whole device, to nothing ([Vulkan](#vulkan) has the table). When the conditions end, `resumeTransition` brings the scene back: a **background fade** in
(the default), the **XMB's start**, as the console comes back from a game, or **at once**. A stop for the GPU's memory skips the transition
and always hides the window, and waits until the GPU's memory is ten points under the threshold before it comes back.
While the settings show, any action only limits the frame rate, so that they can still be used.

`neighbour.rs` looks four times a second on a thread of its own: the foreground window, the windows over the scene's
monitor (cloaked, minimized and click-through overlay windows aside), `SHQueryUserNotificationState` for programs
with the screen to themselves, the process list every two seconds, the battery every five. The GPU's memory comes
from the system's counters, Task Manager's, for the adapter the app draws on; opening them costs a fifth of a second
once, each read a few tens of microseconds. Windows only.

## CPU

The Resources tab's CPU section (`cpu.rs`), applied at start-up and whenever it changes:

- `cpuPlacement`: **Efficiency cores** (the default) keeps the app's threads on a hybrid CPU's E-cores; **One cache
  cluster** keeps them on cores sharing one cache - one cluster of E-cores sharing an L2, or one of several L3s (AMD's
  CCDs); **System decides** leaves them alone. Windows takes it as the process's default CPU sets, a soft affinity
  that every thread follows, the driver's too, unless those cores are taken; Linux as the threads' affinity, from
  `/sys` (`cpu_atom`, or each CPU's capacity or top clock, and its caches); macOS, which takes no cores, as the main
  thread's Background quality of service, which Apple Silicon keeps to its E-cores. A CPU with one class of core has
  no efficiency cores to keep to, and the panel and the overlay say so.
- `cpuEcoQoS` (on): Windows 11's EcoQoS, the process marked as one whose speed matters less than its power - the
  scheduler then prefers the E-cores and lower clocks.
- `cpuPriority` (Auto): normal while Nagare or the desktop has the keyboard, below normal while another program has
  it (Windows; normal elsewhere); or always normal, or always below normal, when every program at normal priority runs
  first when they compete.

Affinity for the caches' sake buys little here. A frame's CPU work is about 0.6 ms in one thread, every 16 ms; between
frames the thread sleeps, and whatever runs meanwhile evicts its data from the core's L1 and L2 anyway, so keeping it
on one core saves nothing measurable, while a hard affinity would keep it waiting when that core is busy (a game's
threads, say). What placement does buy is the kind of core and the cache it shares: on a hybrid CPU the E-cores do the
same work at a fraction of the power, and a cluster of four shares its L2, so keeping to one cluster keeps the wave's
mesh (512 KB) and the particles' pool there between frames. The overlay shows which CPUs the main thread ran on.

## Vulkan

The Resources tab's **renderer** (or `--renderer` for a run) draws the scene with Vulkan, the default, through
[wgpu](https://wgpu.rs) - on macOS, which has no Vulkan of its own, with Metal; with OpenGL, the original renderer; or
with **Software**, the system's software rasteriser on the CPU (WARP, through Direct3D 12, on Windows; a CPU Vulkan
device such as lavapipe elsewhere, if one is installed) - for a PC whose GPU draws neither, at about 14 frames a second
at 1280 × 720 on six cores. Changing it makes the window and its renderer again, behind the icons again if the scene was
there; the wave, the particles and the backdrop go on where they were.

**tripleBuffering** (Resources; Vulkan and the software renderer) lets the display queue a finished frame ahead of the
one it shows - wgpu's frame latency of 2, three images in the swap chain - so that a frame held up for less than a
frame's time still comes on time, at the cost of that frame in latency; off, the latency is 1. Auto (the default) keeps
it on while the frame rate is 30 or more and off below 20, where a queued frame would make the settings slow to answer.
It acts only while frames wait for the display (no cap, or a cap at the refresh rate): a lower cap paces each frame
itself, with time to spare. OpenGL's buffering is the driver's. **tripleBufferingMusicOff** (Yes; shown while
tripleBuffering is On or Auto) turns it off while the music theme plays, so that what the music moves - the wave's
brightness, the camera's bounces - comes a frame closer to what is heard.

**renderScale** (Resources, 0.25 to 1) draws the scene smaller and stretches it over the window, for a GPU - or a CPU -
that cannot keep up: under OpenGL into the frame's lower left, copied out and stretched (`antialias::Upscale`), under
Vulkan into a canvas that much smaller (it multiplies `supersample`). The panel, the warning and the intro are drawn at
the window's full size. **softwareLite** (on by default) leaves the heavier features out while the software renderer
draws, chosen or fallen back to, without touching their settings: supersampling, maru-chan's supersampling, motion blur
and scene light, the disco ball's supersampling, beams and floor, sharpening, FXAA, the settings' blur, renderScale above 0.5, and gpuWave (the spline mesh built on the CPU). The
panel strikes those settings through and says so at its top.

**When one fails.** The renderer asked for is tried first, then the others in turn: Vulkan, OpenGL, software. A
renderer fails when it cannot start - no device, a driver that takes no program (every resource is made under wgpu's
error scopes; OpenGL's start-up, panics and all, is caught) - and, under Vulkan, when its device is lost or reports an
error while drawing: the next one takes over at once. spline.elf on the GPU is made apart: where it cannot run, the CPU
builds the wave, and only it. The panel's top says, in amber, what failed and what draws. `NAGARE_FAIL` makes them
fail on purpose, to try this: `vulkan`, `opengl`, `software`, `gpuwave`, `lost` (the device two seconds in), `lostonce` (only the first device, so that the renderer's return can be watched;
`NAGARE_RETRY_SEC` shortens the 5 minutes before it is tried again) or
`panic` (the whole app three seconds in, for the watchdog), separated by commas. `NAGARE_SUBFRAMES=n` checks the motion blur's sub-frames (see Motion blur), and `NAGARE_PANEL_SCROLL=points` holds the panel scrolled that far down, to shoot a part of it.

It draws what OpenGL draws, with the same programs: `vk/glsl.rs` takes each program's GLSL from the OpenGL modules and
rewrites it for naga's front end - its loose uniforms into one std140 block, its samplers split in two, its inputs
and outputs given locations, its clip depth from -w..w to 0..w - and turns its y over: wgpu follows WebGPU, which puts
NDC's y = +1 in an image's first row where OpenGL puts y = -1, so turned over every image lies in memory as OpenGL's
does, and the programs' screen-space reads (the wave's light a quarter of a pixel up, the dither's texels) land where
OpenGL's do. The uniforms are worked out by the same functions. The frame is drawn into a canvas laid out as OpenGL's
framebuffer, which presenting turns over onto the screen, then egui paints over it. A test of `vk/glsl.rs` runs every
program through naga and checks the block's offsets against naga's own. Against OpenGL, over nine test frames (the
default bloom, the console's glare, a tonemapper; at 1, 6 and 20 s), at most 6 of the 921,600 pixels differ, each by one step in 255, and one frame is the same to the byte - the drivers' compilers rounding a few floats differently.

**spline.elf on the GPU.** `gpuWave` (on) runs the console's SPU program on the GPU, as compute passes
(`vk/wave_gpu.rs`): the deformation of the 361 points through the lattice and the matrix, then the bicubic surface,
128 × 128 vertices with their normals, into the buffer the wave is drawn from. What ran on the PPU stays on the CPU -
the lines' springs, what the task receives, the lattice, the matrix - and the particles' emitter, which reads only the
handful of vertices it picks each frame, works those out itself (`Wave::vertex`), in the same order as the surface,
so it emits from the same points to the bit. Where the GPU has 64-bit floats the passes work in them, as the CPU does,
and the frames come out identical to the CPU's surface; elsewhere they work in single precision. With it, the CPU no
longer builds the 16384 vertices nor uploads their 512 KB every frame.

What stays on the CPU, and why: the lines' step is a chain of updates in order (each spring's pull lands before the
next is worked out, in single precision, and the noise is a counter) over 361 points, a few microseconds; and the
particles' task (`particles.elf`), the other SPU program, hands every slot that dies back to a free list, in order,
which the emitter pops in the next step - on the GPU the CPU would wait for that list every step, a frame's latency or
a stall, or the emitter and its random numbers would have to move too, and be verified against the console again. It
costs about 0.15 ms of the frame.

The costs, from `--test --fps 60 --report` runs (release build with PGO, 1280 × 720, RTX 2070 SUPER, three runs of 40
scene seconds each), in milliseconds per frame on the main thread, with the process's CPU and the driver's busiest
thread as a share of one core:

| Renderer                 | Wave sim | Wave draw | Particle sim | CPU work (p99) | Process CPU | Driver thread | GPU      |
| ------------------------ | -------- | --------- | ------------ | -------------- | ----------- | ------------- | -------- |
| OpenGL                   | 0.326    | 0.056     | 0.153        | 0.559 (0.914)  | 9.8%        | 5.1%          | 1.27 ms  |
| Vulkan, wave on the CPU  | 0.317    | 0.333     | 0.152        | 0.855 (1.234)  | 11.4%       | 3.1%          | 1.09 ms  |
| Vulkan, wave on the GPU  | 0.030    | 0.332     | 0.171        | 0.587 (0.940)  | 8.1%        | 2.3%          | 1.34 ms  |

Moving spline.elf to the GPU takes its 0.3 ms off the CPU for about 0.1 ms of GPU time, in double precision on a GPU
that runs it at a 32nd of its single-precision rate. What Vulkan saves the driver - fewer, larger submissions, no
512 KB upload a frame - wgpu spends again on the main thread: it validates and tracks every one of the frame's 30-odd
render passes, about 0.3 ms. So the process as a whole costs a little less CPU under Vulkan with the GPU's wave than
under OpenGL, and the main thread about the same. Frames are as even: 16.667 ms, 99% within 17.1 ms (OpenGL 17.3).
Under Vulkan, which has no swap interval, a rate that divides the refresh is kept by deadlines a period apart, each
frame shown at the refresh after it.

GPU memory, as Task Manager counts it (`--test`, no panel):

| Window      | OpenGL drawing | stopped | Vulkan drawing | stopped |
| ----------- | -------------- | ------- | -------------- | ------- |
| 1280 × 720  | 44 MB          | 18 MB   | 69 MB          | 0 MB    |
| 2560 × 1440 | 146 MB         | 64 MB   | 198 MB         | 0 MB    |

Vulkan holds more while drawing: the canvas it turns over onto the screen and a third swapchain image, about 10 bytes
a pixel, and 14 MB a device costs; a stop drops the device, and with it everything.

## Testing and profiling

`--test` makes a run repeatable, for profiling and for comparing what is drawn: a 1280 × 720 window (or
`--size WxH`), every setting at its default, the config neither read nor written, the particles seeded, autoMusic and
the Resources tab's conditions off, and a clock that moves exactly `--test-dt` (1/60 s) each frame from `--test-date`
(2026-06-15T20:30:00), whatever the wall clock does - so the same frame number always draws the same image, to the
byte. Frames come as fast as they can unless `--fps` caps them; `--exit-after` and `--shot` count the scene's
seconds, not the wall's.

| Option             | What it does                                                                               |
| ------------------ | ------------------------------------------------------------------------------------------ |
| `--test`           | The repeatable run above.                                                                  |
| `--test-date T`    | The date and time the scene starts at, as `2026-01-10T07:00:00`.                           |
| `--test-dt S`      | The seconds each frame moves the clock on.                                                 |
| `--size WxH`       | The window's size in pixels.                                                               |
| `--set KEY=VALUE`  | Sets any numeric setting by its panel name (`glareMode=0`, `tonemap=2`), as if by hand; `musicPlaying=1` sets the scene's music playing, as the panel's Music does (for the music theme, the disco ball). |
| `--shot SEC=FILE`  | Saves the frame drawn at that scene time as a PNG (the scene, and the panel with `--ui`, without the overlay). Repeatable. |
| `--exit-after SEC` | Exits at that scene time.                                                                  |
| `--report FILE`    | On exit, writes each stage's CPU time and each pass's GPU time (mean, median, 95th and 99th percentiles, worst), the process's CPU time and each thread's, and each frame's interval and stages (`each_frame`, where a spike lands), as JSON, and prints a summary. |
| `--warmup SEC`     | The scene seconds left out of the report at the start (1).                                 |
| `--set-at SEC:KEY=VALUE` | Sets one at that scene time, as the panel would (`renderer=1` switches to Vulkan mid-run; `musicPlaying` too). Repeatable. |
| `--renderer API`   | Draws with Vulkan, OpenGL or software.                                                     |
| `--ui`             | Shows the panel in a test.                                                                 |

```bash
nagare --test --shot 8=a.png --exit-after 8                      # the frame at 8 s, the same every run
nagare --test --fps 60 --exit-after 20 --warmup 2 --report r.json  # a minute's costs, paced as in use
nagare --test --ui --tab Panel --shot 6=panel.png --exit-after 6   # the panel, for comparing its look
nagare --test --renderer vulkan --shot 8=b.png --exit-after 8      # the same frame under Vulkan
nagare --test --set-at 3:renderer=1 --shot 5=v.png --exit-after 6  # switching to Vulkan mid-run
```

For CPU costs, pace the run (`--fps 60` or a divisor of the refresh rate): unpaced, the driver's thread competes with
the main thread for the CPU and the numbers wander.

### Build and PGO

The release profile is for working on Nagare: no LTO, 16 codegen units and incremental compilation. Rebuilding after
an edit to `main.rs` takes about 4 s, against 12 s without incremental compilation and 54 s with the thin LTO it
used before (measured 2026-10-28), for some of the CPU's frame time; the images are the same to the byte. (rust-lld
linked slower here, and was left out.) The PGO builds set fat LTO, one unit and no incremental compilation for
themselves (`CARGO_PROFILE_RELEASE_*`), so a release keeps the last few
percent. `cargo xtask pgo` builds with profile-guided optimisation:
1. an instrumented build (`cargo xtask pgo-instrument`);
2. twelve `--test` runs with it, through both glare modes, every tone curve, auto exposure, the noise, the
   desaturation, the overlay and the day cycle, under OpenGL and under Vulkan (`cargo xtask pgo-train`);
3. LLVM merges what they recorded into `pgo\nagare.profdata`, with the compiler that recorded it in
   `pgo\nagare.profdata.txt` (`cargo xtask pgo-merge`; it needs `rustup component add llvm-tools`, which
   `rust-toolchain.toml` lists);
4. the release build is optimised with it (`cargo xtask pgo-build`).

The main thread's CPU work per frame, at 60 fps on 1280 × 720, four runs each:

| Build                     | Wave sim | Particle sim | CPU work |
| ------------------------- | -------- | ------------ | -------- |
| thin LTO (the profile)    | 0.473 ms | 0.172 ms     | 0.720 ms |
| fat LTO, 1 codegen unit   | 0.441 ms | 0.172 ms     | 0.687 ms |
| fat LTO + PGO             | 0.330 ms | 0.171 ms     | 0.575 ms |

The repository's release workflow (`.github/workflows/native-release.yml`) runs only `cargo xtask pgo-build`, since
the training draws the scene and needs a GPU, which the workflow's machines lack. Record the profile on a PC and
commit `pgo\`. A profile fits only the compiler that recorded it, which `rust-toolchain.toml` pins, and the code it was
recorded on; code changed since merely goes unoptimised, so each commit records it again (`cargo xtask ready`).

#### `cargo xtask`

The build tasks are a small Rust program, `xtask/` (standard library only), which Cargo runs through the alias in
`.cargo/config.toml`: they work wherever Cargo does, with nothing else installed. From `native/`:

| Task | What it does |
| --- | --- |
| `cargo xtask build` | the quick release build (thin LTO) |
| `cargo xtask check` | what CI checks: clippy for Windows, Linux and macOS, warnings as errors, then the tests |
| `cargo xtask pgo` | the whole PGO pass above, about 20 minutes; or step by step: `pgo-instrument`, `pgo-train [SECONDS]`, `pgo-merge`, `pgo-build` |
| `cargo xtask installer [--no-build] [--install]` | the installer, from a fresh build or (`--no-build`) the exes already built; `--install` installs it silently |
| `cargo xtask shots DIR [--exe EXE] [-- ARGS...]` | the regression shots: OpenGL, Vulkan, Vulkan supersampled and OpenGL without the enhancements, at 6 s and 25 s |
| `cargo xtask compare DIR1 DIR2` | two shots folders compared byte for byte |
| `cargo xtask ready` | everything before a commit: `check`, `pgo`, then `installer --no-build --install` |

A Nagare running from `target\release` would lock its exe against the build; the tasks move such an exe aside (to
`.old`, where it keeps running) rather than stopping it.

Every build draws the same image to the byte. About as much CPU again goes to the OpenGL driver's own thread, which
carries out the frame's GL calls - the wave's mesh, 512 KB uploaded every frame, among them; under Vulkan with the
wave on the GPU both go ([Vulkan](#vulkan)).

## Layout

Each module up to `main.rs` is a port of one page file. The JS files and the reverse-engineering notes stay the
source of truth: the constants, and the comments naming their firmware addresses, are carried over as they stand.

| Rust                       | Page                                                         |
| -------------------------- | ------------------------------------------------------------ |
| `src/wave.rs`              | `wave-reverse.js`                                            |
| `src/spline.rs`            | `spline.js`                                                  |
| `src/backdrop.rs`          | `backdrop.js` (Auto only; the old gradient presets are left out) |
| `src/months.rs`            | `background-months.js` (generated from it)                   |
| `src/postprocess.rs`       | `postprocess.js`                                             |
| `src/particles_sim.rs`     | `particles-reverse.js`                                       |
| `src/particles.rs`         | `particles.js`                                               |
| `src/scene.rs`             | `scene-themes.js`                                            |
| `src/settings.rs`          | `spline-settings.js`, `particles-settings.js`                |
| `src/input.rs`             | `xmb-input.js`                                               |
| `src/panel.rs`             | `settings-panels.js`, and the app's own controls at its top  |
| `src/main.rs`              | `index.html`                                                 |
| `src/auto_exposure.rs`     | the app's own: auto exposure                                 |
| `src/config.rs`            | the app's own: the config file                               |
| `src/perf.rs`              | the app's own: the perf overlay                              |
| `src/steamboat.rs`         | the app's own: the perf overlay's steamboat, the music drawn   |
| `src/maru.rs`              | the app's own: maru-chan, the 3D boat on the wave             |
| `src/disco.rs`             | the app's own: the disco ball, heavy bass brings it down      |
| `src/warning.rs`           | the app's own: the photosensitive epilepsy warning            |
| `src/intro.rs`             | the app's own: the first start's benchmark screen and card    |
| `src/machine.rs`           | the app's own: the drivers seen and the benchmark's result    |
| `src/antialias.rs`         | the app's own: FXAA, and supersampling under Vulkan           |
| `src/sharpen.rs`           | the app's own: sharpening (CAS) over the finished frame    |
| `src/wallpaper.rs`         | the desktop embedding (Windows only)                         |
| `src/tray.rs`              | the tray icon (Windows only)                                 |
| `src/autostart.rs`         | the app's own: Start with Windows, for an installed copy     |
| `src/settings_window.rs`   | the app's own: the settings in a window of their own         |
| `src/media.rs`             | the app's own: the system's media sessions, for autoMusic    |
| `src/audio.rs`             | the app's own: listening to the output in the music scene    |
| `src/presence.rs`          | the app's own: the session locked or the display off (Windows only) |
| `src/neighbour.rs`         | the app's own: other programs' conditions (Windows only)     |
| `src/cpu.rs`               | the app's own: where the threads run, and what each costs    |
| `src/blur.rs`              | the app's own: the settings' blurred background              |
| `src/sim.rs`               | `index.html`'s frame: the wave, its fade and the particles, whichever API draws |
| `src/vk/`                  | the app's own: the Vulkan renderer (wgpu), below             |
| `src/logo.rs`              | the app's own: the logo's sizes, as `assets/logo` holds them |
| `src/watchdog.rs`          | the app's own: starting the watchdog, and the crash log      |
| `src/bin/watchdog.rs`      | `nagare-watchdog`, which starts the app again should it crash |
| `src/bin/customize/`       | `nagare-customize`: the app's and the installer's icons, the installer's look and build |
| `build.rs`                 | the logo's icon, into the Windows executables                |

The simulations keep the page's number types (its `Float32Array`s are `f32`, its other numbers `f64`), so they round
where the page does. Two things differ from WebGL. Desktop GL only has a fixed primitive-restart index from 4.3 on,
so the wave's strips are joined with degenerate triangles, which draw the same. And desktop GL does not zero new
textures, so every render target is cleared when it is made.

## The watchdog

`nagare-watchdog` starts the app again should it crash. The app starts it at start-up, from its own folder, unless one
runs already (or with `--test` or `--no-watchdog`), and hands it its process, its path and its arguments. The
watchdog waits for the app to end: an exit as asked (0) or Task Manager's End task (1) ends the watchdog too; anything
else - a panic, an abort, an exception the system caught - and it starts the app again with the same arguments two
seconds later, and watches that one. Three crashes in two minutes and it gives up. It logs what it does in
`watchdog.log`, and the app leaves each panic's message and backtrace in `crash.log`, both beside the config.
Windows only; the Resources tab says what became of it.

## The customizer

`nagare-customize` (`cargo run --release --bin nagare-customize`) designs what Nagare looks like outside its scene, in
three tabs, each showing what it makes as it changes and writing it with **Render**, its design beside it, which it
opens again. Everything is drawn on the CPU, in linear light, several samples a pixel, each size at its own size.

- **App icon** (`logo.rs`), the logo: a letter - its font, from the system's fonts, its size and place, a gradient,
  gloss and an outline - a drop shadow (colour, opacity, blur, intensity, offset) and the top of a Phong-shaded sphere
  it stands on (its place and size, colour, the light's direction, ambient, diffuse, specular and shininess, a rim
  light), and a rounded plate behind it all, if wanted. Render writes into `assets/logo` every size from 16 to 512
  pixels, `nagare.ico` (16 to 256) and `design.json`. The app takes them in at compile time (`src/logo.rs`): the
  tray's icon at the system's small-icon size, the windows' icons, and the logo right-clicking shows; `build.rs` puts
  `nagare.ico` into the executables, as a resource file it writes itself, so no resource compiler is needed.
- **Installer icon** (`package.rs`), the installer's: a glass box seen from above and to one side, its top and two
  sides, a nod to the console's `.pkg` packages. Each side shows the logo (the App icon tab's, as it stands), any text
  ("PKG" on the right to begin with; a symbol or emoji its font lacks, ★ say, comes from Windows' Segoe UI Symbol or
  Emoji, in the text's one colour), any PNG or nothing, at its own size and place (Swap left and right trades the two sides' settings); the box's turn, tilt and
  proportions, the glass's colours, opacity and light on each face, its reflection, its edges and the hidden ones
  seen through it, a strip of tape across the top and the shadow are knobs too. Render writes into
  `installer/theme` every size, `setup.ico` and `box.json`.
- **Installer** (`installer.rs`), the setup wizard: light, dark or as Windows is, Inno Setup's built-in styles, its
  background colours, and its pictures - the welcome and finish pages' tall one (a shot of the scene, darker at the
  foot, with the box - or, transparent, the logo alone), the box in the other pages' corner, and a wide shot behind
  every page, at an opacity. **Take shots** runs the release build for the two shots (`--test` runs, the same every
  time), each at a date and time, and seconds into it, of its own, with auto exposure or without, and maru-chan held
  at a place across it of one's choosing (the app's hidden `maruAt`) or away, either alone or both; the tall one is cut
  out of a frame of the wide one's shape, where its **framing** says, so that its auto exposure meters the whole scene
  and it can take the wave's fuller part. **Render settings** sets what both are
  drawn with - the renderer, FXAA, supersampling, sharpening, grain, maru-chan's size, and any other setting as
  `--set` takes it. Render writes the
  pictures at the sizes Setup picks from by DPI, `theme.json` and `theme.iss`, the directives `nagare.iss` takes in.
  Any picture can be replaced by one's own (keep my own pictures stops Render writing over them).
  **Build the installer** and **Build and install** run `cargo xtask installer`, its output shown as it comes.

The font lists offer every font installed, Windows' and the user's own: each .ttf and .otf, and each face of a
collection (.ttc, Yu Gothic's say) by its own name, saved as the file's name and the face's index (`YuGothB.ttc#1`).

Without a window: `--list-fonts` (what the font lists offer), `--render` (the app's icons), `--render-box`, `--take-shots` and `--render-installer`, each from the
designs saved; `--page box` or `--page installer` opens the window on that tab.

## Wallpaper mode

Explorer's `Progman` window, sent its undocumented `0x052C` message, splits the desktop into the icons
(`SHELLDLL_DefView`) and a `WorkerW` that draws the wallpaper behind them. Up to Windows 11 23H2 that `WorkerW` is a
top-level window, and the scene becomes its child. From 24H2 on it is a child of `Progman`, so the scene becomes
`Progman`'s child, placed just below the icons. It is made a layered window at full opacity, because `Progman` there
has no redirection bitmap and draws nothing of a plain child. The 24H2 path is the one tested, on build 26300.
