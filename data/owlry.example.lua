-- ~/.config/owlry/owlry.lua — example configuration.
--
-- Drop this file in place to switch from config.toml to the Lua config
-- layer. owlry.lua takes precedence over config.toml when both exist
-- (info-logged at daemon start). The daemon watches this file and
-- hot-reloads on save; broken edits surface as desktop notifications
-- and keep the previous state alive.
--
-- Spec: https://somegit.dev/Owlibou/owlry/src/branch/main/docs/lua-api.md

-- ──────────────────────────────────────────────────────────────────────
-- 1. Global settings.
--    Every key here is OPTIONAL — defaults apply for anything you omit.
-- ──────────────────────────────────────────────────────────────────────
owlry.set {
  theme           = "owl",     -- bundled themes: owl, catppuccin-mocha,
                               -- nord, rose-pine, dracula, gruvbox-dark,
                               -- tokyo-night, solarized-dark, one-dark,
                               -- apex-neon. Plus any *.css under
                               -- ~/.config/owlry/themes/.
  width           = 850,
  height          = 650,
  font_size       = 14,
  border_radius   = 12,
  -- terminal     = "kitty",   -- omit to auto-detect ($TERMINAL → xdg-
                               -- terminal-exec → DE-native → fallback).
  use_uwsm        = false,     -- launch desktop entries via `uwsm app --`
                               -- for proper systemd session integration.
  show_icons      = true,
  max_results     = 100,
  frecency        = true,      -- boost recently / frequently launched items
  frecency_weight = 0.3,       -- 0.0 = off, 1.0 = strong
  frecency_max_entries  = 5000, -- cap retained entries (0 = unbounded)
  frecency_max_age_days = 180,  -- drop entries older than this (0 = no age pruning)
  frecency_min_launches = 3,    -- launches that exempt an old entry from age pruning
  search_engine   = "duckduckgo",  -- google | duckduckgo | bing |
                                   -- startpage | searxng | brave |
                                   -- ecosia, or a custom "{query}" URL.

  -- Root dirs for file search (the :file prefix or the / alias). "~/" is
  -- expanded; non-existent entries are dropped. Empty (the default) searches
  -- $HOME. File search is opt-in only — it never runs on a bare query.
  -- filesearch_roots = { "~/Documents", "~/code" },
}

-- ──────────────────────────────────────────────────────────────────────
-- 2. Enabled providers.
--    Omit this call to enable every built-in provider (the default).
--    Listing a subset disables anything not mentioned. Pre-v2 aliases
--    (sys / system → power; uuctl → systemd) still work.
-- ──────────────────────────────────────────────────────────────────────
owlry.providers {
  "app", "cmd",                                -- core launchers
  "calc", "conv",                              -- = / > triggers
  "power",                                     -- shutdown / reboot / lock
  "systemd",                                   -- user units (:uuctl)
  "ssh", "websearch", "filesearch",            -- search-style providers
  "emoji", "clipboard",                        -- pickers
}

-- ──────────────────────────────────────────────────────────────────────
-- 3. Tab bar order (subset of providers).
--    Omit to give every enabled provider a tab. Order is preserved;
--    Ctrl+1 jumps to the first entry, Ctrl+2 the second, and so on.
-- ──────────────────────────────────────────────────────────────────────
owlry.tabs { "app", "cmd", "uuctl" }

-- ──────────────────────────────────────────────────────────────────────
-- 4. Theme overrides.
--    owlry.theme("name") picks a bundled / user theme.
--    owlry.theme {} layers individual colour overrides on top.
--    Both forms compose — call as many times as you like.
-- ──────────────────────────────────────────────────────────────────────
-- owlry.theme {
--   background       = "#1e1e2e",
--   background_secondary = "#313244",
--   border           = "#45475a",
--   text             = "#cdd6f4",
--   text_secondary   = "#a6adc8",
--   accent           = "#cba6f7",
--   accent_bright    = "#f5c2e7",
--   -- Per-provider badges (each is optional):
--   badge_app        = "#a6e3a1",
--   badge_cmd        = "#fab387",
--   badge_power      = "#f38ba8",
--   badge_uuctl      = "#9ece6a",
-- }

-- ──────────────────────────────────────────────────────────────────────
-- 5. Named profiles, selected by `owlry --profile <name>`.
--    Each profile overrides the default enabled-provider set for that
--    launch but inherits owlry.set / owlry.theme / owlry.tabs.
-- ──────────────────────────────────────────────────────────────────────
-- owlry.profiles {
--   dev     = { "app", "cmd", "ssh" },
--   media   = { "emoji", "clipboard" },
--   minimal = { "app" },
-- }

-- ──────────────────────────────────────────────────────────────────────
-- 6. User-defined providers.
--    Register your own search source. Every field except `id` and
--    `items` is optional. `items` is called once at startup and the
--    results cached (dynamic per-keystroke providers land in 2.2).
-- ──────────────────────────────────────────────────────────────────────
-- owlry.provider {
--   id          = "hs",
--   prefix      = ":hs",
--   tab_label   = "Shutdown",
--   icon        = "system-shutdown",
--   search_noun = "shutdown actions",
--   items = function()
--     return {
--       { name = "Lock",     command = "hyprlock" },
--       { name = "Shutdown", command = "systemctl poweroff" },
--       { name = "Reboot",   command = "systemctl reboot" },
--     }
--   end,
-- }

-- ──────────────────────────────────────────────────────────────────────
-- 7. Host helpers (use inside provider `items` callbacks).
--    Available under owlry.util.*:
--      shell(cmd)           -> string  (stdout, trimmed)
--      shell_lines(cmd)     -> {string}
--      read_file(path)      -> string | nil
--      glob(pattern)        -> {string}  (~ expansion supported)
--      env(name, default?)  -> string | nil
--      hostname()           -> string
-- ──────────────────────────────────────────────────────────────────────
-- owlry.provider {
--   id = "docker-ps",
--   prefix = ":docker",
--   items = function()
--     local lines = owlry.util.shell_lines(
--       "docker ps --format '{{.Names}}\t{{.Image}}'"
--     )
--     local items = {}
--     for _, line in ipairs(lines) do
--       local name, image = line:match("([^\t]+)\t(.+)")
--       if name then
--         items[#items + 1] = {
--           name = name,
--           description = image,
--           command = "docker exec -it " .. name .. " bash",
--           terminal = true,
--         }
--       end
--     end
--     return items
--   end,
-- }
