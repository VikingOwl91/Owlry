# Creating media for Owlry

All screenshots and recordings must come from the real application. Do not use
AI-generated images, synthetic UI frames, or mocked application interactions.

## Screenshots

```bash
# Install on Arch
sudo pacman -S grim slurp

# Region
grim -g "$(slurp)" assets/screenshot.png

# Whole output
grim assets/screenshot.png
```

Store README assets in `assets/`. The static main-window image is
`assets/owlry-screenshot.png`.

## Demo GIF

The README demo is a short loop recorded from a real Owlry 2.8.0 session. It
must not be recorded from the active desktop.

### Isolation

Use a nested headless Sway session with its own runtime directory, home, and
XDG directories. Do not point `WAYLAND_DISPLAY` at the active session.

```bash
export DEMO=/tmp/owlry-demo
export XDG_RUNTIME_DIR="$DEMO/runtime"
export WAYLAND_DISPLAY=wayland-owlry-demo
mkdir -p "$DEMO"/{runtime,config/owlry,data,cache,home}
chmod 700 "$DEMO/runtime"

WLR_BACKENDS=headless \
WLR_LIBINPUT_NO_DEVICES=1 \
WLR_RENDERER=pixman \
sway --config "$DEMO/sway.conf"
```

`$DEMO/sway.conf`:

```text
output HEADLESS-1 resolution 1000x760
exec /tmp/owlry-demo/start-owlry.sh
```

The start script runs the daemon and UI with the demo environment:

```bash
#!/usr/bin/env bash
set -euo pipefail
export DEMO=/tmp/owlry-demo
export XDG_RUNTIME_DIR="$DEMO/runtime"
export XDG_CONFIG_HOME="$DEMO/config"
export XDG_DATA_HOME="$DEMO/data"
export XDG_CACHE_HOME="$DEMO/cache"
export HOME="$DEMO/home"
export XDG_CURRENT_DESKTOP=sway

owlry -d &
sleep 1
owlry
```

Keep the system `XDG_DATA_DIRS` so system-wide `.desktop` applications remain
visible. A temporary `XDG_DATA_HOME` and `HOME` exclude personal application
entries.

### Demo configuration

Use a dedicated `$DEMO/config/owlry/owlry.lua`. Keep it small and avoid
private providers:

```lua
owlry.set {
  theme = "owl",
  width = 1000,
  height = 760,
  font_size = 15,
  show_icons = true,
}

owlry.providers { "app", "cmd", "calc", "conv", "power" }
owlry.tabs { "app", "cmd", "power" }
```

Do not enable clipboard, SSH, filesearch, or websearch for a public recording.
They can expose personal data or make network requests.

### Record and drive

Record the headless output, then type with `wtype` in the same environment:

```bash
wf-recorder -r 30 -f "$DEMO/demo.mkv"
```

```bash
wtype "fire"
wtype -k BackSpace
wtype -M ctrl -k a -m ctrl
wtype "= 1920/1080"
```

Use short `sleep` pauses between scenes so the result is readable. Stop
`wf-recorder` with `Ctrl+C` after the final hold.

Headless Sway has no pointer device, and a layer-shell surface can lose
keyboard focus when a short-lived virtual keyboard disconnects. Start the UI
inside the capture and verify focus before the storyboard. If focus is
unreliable, keep one virtual keyboard connection alive for the whole take. Do
not record a take whose focus cannot be verified.

### Storyboard

| Time | Action | Screen |
|---|---|---|
| 0.0 to 1.2 s | Launcher opens with an empty query | "All" tab, search field focused |
| 1.2 to 3.8 s | Type `fire` | Results narrow; Firefox highlighted |
| 3.8 to 4.6 s | Hold | Viewer can read the result |
| 4.6 to 6.8 s | Select all, type `= 1920/1080` | Calculator result |
| 6.8 to 7.6 s | Hold | Viewer can read the result |
| 7.6 to 9.8 s | Clear, type `> 20 km to mi` | Converter result |
| 9.8 to 10.6 s | Hold | Viewer can read the result |
| 10.6 to 12.8 s | Clear, type `:power` | Power actions; highlight Lock Screen |
| 12.8 to 13.5 s | Clear to empty query | Returns to the starting view; hold, then loop |

Do not activate Lock Screen, Suspend, or any destructive action during
capture. Do not launch applications that open windows over the launcher.

### Convert

```bash
ffmpeg -ss 0.5 -t 13.5 -i "$DEMO/demo.mkv" \
  -vf "fps=15,scale=1000:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192:stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3" \
  -loop 0 assets/owlry-demo.gif
```

Targets, in order of importance:

- Readable text at the rendered width.
- Smooth motion and a seamless loop.
- 1000x760, 15 fps, about 13.5 seconds.
- Under 1.5 MB if possible. Reduce colors or scale if the file grows beyond
  that, but do not trade away legibility.

If `gifsicle` is installed, `gifsicle -O3 --lossy=40` can shrink the result.
It is optional; the ffmpeg palette pass is enough.

### Review

```bash
ffprobe -v error -select_streams v:0 \
  -show_entries stream=width,height,avg_frame_rate,duration \
  -show_entries format=size assets/owlry-demo.gif

# Extract frames for inspection
ffmpeg -v error -ss 2.5 -i assets/owlry-demo.gif -frames:v 1 /tmp/frame.png
```

Inspect the frames and the final loop before replacing the committed asset.
Keep the previous GIF until the new one has been captured and reviewed.

## Privacy checklist

- The recording runs in a nested headless compositor, not the active session.
- `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_CACHE_HOME` point at
  temporary directories.
- Clipboard, SSH, filesearch, and websearch providers are disabled.
- No notifications, chat windows, file paths, or host names are visible.
- Every scene is reviewed before the GIF is committed.

If the headless compositor cannot host the Layer Shell window, stop and report
the failure. Do not switch to the active desktop.
