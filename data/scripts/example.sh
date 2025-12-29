#!/bin/bash
# Example Owlry Script
# Copy to: ~/.local/share/owlry/scripts/
#
# Scripts in the scripts directory appear in Owlry search results.
# They are executed when selected.
#
# Naming convention:
#   The filename (without extension) becomes the display name.
#   Example: "system-update.sh" shows as "Script: system-update"
#
# Tips:
#   - Make scripts executable: chmod +x script.sh
#   - Use descriptive names for easy searching
#   - Scripts can launch GUI apps, run terminal commands, etc.

# Example: Show a notification
notify-send "Owlry" "Hello from example script!"

# Example: Open a URL
# xdg-open "https://example.com"

# Example: Run a terminal command (set terminal: true in owlry if needed)
# echo "Script executed at $(date)"
