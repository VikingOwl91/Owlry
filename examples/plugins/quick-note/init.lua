-- Quick Note Plugin for Owlry
--
-- This plugin demonstrates:
-- 1. Dynamic providers (query-based)
-- 2. Using owlry.fs for file operations
-- 3. Using owlry.path for directory helpers
-- 4. Using owlry.json for data storage

owlry.log.info("Quick Note plugin loading...")

-- Get notes file path
local notes_dir = owlry.path.join(owlry.path.data(), "notes")
local notes_file = owlry.path.join(notes_dir, "quick-notes.json")

-- Load existing notes
local function load_notes()
    local content = owlry.fs.read(notes_file)
    if content then
        local data = owlry.json.decode(content)
        if data and data.notes then
            return data.notes
        end
    end
    return {}
end

-- Save notes
local function save_notes(notes)
    local data = { notes = notes }
    local json = owlry.json.encode(data)
    owlry.fs.write(notes_file, json)
end

-- Register a dynamic provider
-- Dynamic providers have a 'query' function that receives user input
owlry.provider.register({
    name = "quick-note",
    display_name = "Quick Notes",
    default_icon = "text-x-generic",

    -- Prefix to activate this provider
    prefix = ":note",

    -- Dynamic query function - called as user types
    query = function(q)
        local items = {}
        local query = q or ""

        -- If query is not empty, offer to save it as a note
        if #query > 0 then
            table.insert(items, {
                id = "save-note",
                name = "Save: " .. query,
                description = "Save this as a new note",
                icon = "document-save",
                -- Special command format - would be handled by plugin action
                command = "PLUGIN:quick-note:save:" .. query,
                tags = { "save", "new" }
            })
        end

        -- Load and display existing notes
        local notes = load_notes()
        for i, note in ipairs(notes) do
            -- Filter notes by query
            if #query == 0 or string.find(string.lower(note.text), string.lower(query)) then
                table.insert(items, {
                    id = "note-" .. i,
                    name = note.text,
                    description = "Saved on " .. (note.date or "unknown"),
                    icon = "text-x-generic",
                    -- Copy to clipboard command
                    command = "echo -n '" .. note.text:gsub("'", "'\\''") .. "' | wl-copy",
                    tags = { "note", "saved" }
                })
            end
        end

        -- If no notes exist, show a hint
        if #items == 0 then
            table.insert(items, {
                id = "hint",
                name = "Type something to save a note",
                description = "Notes are saved to " .. notes_file,
                icon = "dialog-information",
                command = "",
                tags = { "hint" }
            })
        end

        return items
    end
})

owlry.log.info("Quick Note plugin loaded!")
