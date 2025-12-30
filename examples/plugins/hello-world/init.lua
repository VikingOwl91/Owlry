-- Hello World Plugin for Owlry
--
-- This minimal plugin demonstrates:
-- 1. Using owlry.log for logging
-- 2. Registering a static provider
-- 3. Returning items with all available fields

owlry.log.info("Hello World plugin loading...")

-- Register a static provider
-- Static providers have a 'refresh' function that returns items once
owlry.provider.register({
    -- Required: unique provider name (used internally)
    name = "greeting",

    -- Optional: human-readable display name
    display_name = "Hello World",

    -- Optional: icon name (freedesktop icon spec)
    default_icon = "face-smile",

    -- Optional: prefix to trigger this provider (e.g., ":hello")
    prefix = ":hello",

    -- Required for static providers: function that returns items
    refresh = function()
        -- Get username from environment or default to "User"
        local user = os.getenv("USER") or "User"

        return {
            {
                id = "greeting-1",
                name = "Hello, " .. user .. "!",
                description = "A friendly greeting from Lua",
                icon = "face-smile",
                -- Command to run when selected (optional)
                command = "notify-send 'Hello' 'Greetings from Owlry!'",
                -- Whether to run in terminal (optional, default false)
                terminal = false,
                -- Tags for search/filtering (optional)
                tags = { "greeting", "hello", "example" }
            },
            {
                id = "greeting-2",
                name = "Current time: " .. os.date("%H:%M:%S"),
                description = "The current system time",
                icon = "appointment-soon",
                -- Empty command = info only, no action
                command = "",
                tags = { "time", "clock" }
            },
            {
                id = "greeting-3",
                name = "Open Owlry docs",
                description = "Visit the Owlry documentation",
                icon = "help-browser",
                command = "xdg-open 'https://github.com/Owlibou/owlry'",
                tags = { "help", "docs", "documentation" }
            }
        }
    end
})

owlry.log.info("Hello World plugin loaded successfully!")
