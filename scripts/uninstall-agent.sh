#!/bin/sh
# Stops and removes the tiling_window LaunchAgent installed by install-agent.sh.
set -eu

LABEL="com.rodrigopoblete.tiling-window"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
rm -f "$PLIST"

echo "Stopped and removed: $LABEL"
echo "Log kept at: $HOME/Library/Logs/tiling-window.log"
