# Nagare's installer

A Windows installer, made with [Inno Setup 6](https://jrsoftware.org/isinfo.php): one `nagare-setup-VERSION.exe` that
installs Nagare for the user alone, with no administrator, into `%LOCALAPPDATA%\Programs\Nagare` - where Windows
keeps per-user programs - with a Start menu shortcut and an entry in Settings > Apps.

## What you need

- Rust, as for any build of Nagare.
- Inno Setup 6, once:

  ```powershell
  winget install --id JRSoftware.InnoSetup --scope user
  ```

## Building it

From `native\`, in PowerShell:

```powershell
.\installer.ps1             # builds the release exes, then the installer
.\installer.ps1 -NoBuild    # packages the exes already in target\release (after pgo.ps1, for a release)
.\installer.ps1 -Install    # builds, then installs it silently over the installed copy
```

The installer lands in `target\installer\nagare-setup-VERSION.exe`, the version being `Cargo.toml`'s. A Nagare running
from `target\release` is stopped first, since its exe is about to be replaced.

Or from the customizer, `cargo run --release --bin nagare-customize`: its **Installer** tab has **Build the installer**
and **Build and install**, which run the same script and show what it says.

## What it does

- Copies `nagare.exe` and `nagare-watchdog.exe`, and adds a Start menu shortcut, a desktop one if asked, and the entry
  in Settings > Apps, whose install location is what tells the app it is an installed copy (`src/autostart.rs`).
- Offers **Start Nagare when I sign in**, ticked to begin with: the user's own `Run` value, the same one the panel's
  Start with Windows switch writes. Installing again keeps what was chosen last time.
- Stops a running Nagare first, the watchdog before the app. Its finish page offers to start Nagare; a silent install
  instead starts again the Nagare it stopped, if one ran.
- Uninstalling (Settings > Apps) stops Nagare, removes all of that - the start at sign-in too, however it was set -
  and leaves the config in `%APPDATA%\Nagare`, which is the user's.

## Without the wizard

```powershell
nagare-setup-0.1.0.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART        # the tasks chosen last time, or the defaults
nagare-setup-0.1.0.exe /VERYSILENT /TASKS="autostart,desktopicon"      # these tasks and no others
nagare-setup-0.1.0.exe /VERYSILENT /TASKS=""                          # no start at sign-in, no desktop shortcut
nagare-setup-0.1.0.exe /SILENT                                        # a progress window, and no questions
"%LOCALAPPDATA%\Programs\Nagare\unins000.exe" /VERYSILENT              # uninstall
```

`/LOG="file.txt"` writes what it did. `-Install` uses the first line.

## How it looks

`nagare.iss` takes its look from `theme\theme.iss`, beside it, which the customizer's **Installer** tab writes from
`theme\theme.json`. Everything it uses is in `theme\`:

| File                       | What it is                                                     | Sizes Setup picks from          |
| -------------------------- | -------------------------------------------------------------- | ------------------------------- |
| `setup.ico`                | the installer's own icon, the Installer icon tab's glass box   | 16 to 256                       |
| `wizard-*.png`             | the welcome and finish pages' picture, on the left             | 202x386, 336x643, 534x1022      |
| `corner-*.png`             | the other pages' small picture, top right                      | 58, 77, 97, 124, 159            |
| `back-*.png`               | behind every page, at the opacity the tab sets                 | 596x432, 994x720, 1272x922      |
| `scene-tall.png`, `scene-wide.png` | the scene's shots, which the tab makes the pictures from | any                             |
| `box-*.png`, `box.json`    | the box at every size, and its design                          |                                 |

They are templates as much as output: put your own art in place of any of them. Setup takes PNGs with transparency,
any number of sizes, and picks the one that best fits the screen's DPI, so keep each picture's aspect - 164:314 for
`wizard`, square for `corner`, 497:360 for `back`; the names only have to match the patterns. Then tick **keep my
own pictures** in the Installer tab, so that its Render writes only `theme.json` and `theme.iss`. Or keep the tab's
compositing and replace just `scene-tall.png` and `scene-wide.png`: Render cuts them to size and puts the box on.
With **welcome picture transparent**, `wizard-*.png` holds the logo alone, at the box's size and height, over the
page's own colour.

The tab's knobs, and the directives they write:

- **appearance** - `WizardStyle=modern dynamic` (as Windows is), `light` or `dark`;
- **style** - Inno Setup's built-in styles: `windows11`, `polar`, `slate`, `zircon`, or none;
- **no lines between the parts of a page** - `hidebevels`; **style the title bar too** - `includetitlebar`;
- **background colours** - `WizardBackColor`, and `WizardBackColorDynamicDark` for dark as Windows is;
- **pictures** - `WizardImageFile`, `WizardSmallImageFile` (each also as `DynamicDark`, or Setup swaps in dark
  pictures of its own) and `WizardBackImageFile`, with `WizardImageOpacity` and `WizardBackImageOpacity`.

`theme.iss` can be edited by hand too, until the tab next saves. Beyond what the tab offers, Inno Setup also takes a
whole VCL style (`WizardStyleFile=MyStyle.vsf`, and `WizardStyleFileDynamicDark`), made with Delphi's Bitmap Style
Designer, for every control's colours and shapes; see its help, `ISetup.chm`, under [Setup] WizardStyle.

## Signing

The exes and the installer are not signed, so Windows SmartScreen warns about a downloaded installer ("Windows
protected your PC", More info > Run anyway) until they are, with a code-signing certificate and `signtool`, which
Inno Setup can run itself (`SignTool=` in [Setup]).
