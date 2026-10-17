#!/bin/bash
# Atlantis Native.app launcher: asks once for the game folder (atlantis.exe
# and BIGCD*.BIG, or the GOG.com installation) and remembers it.
here="$(cd "$(dirname "$0")" && pwd)"
conf="$HOME/Library/Application Support/Atlantis Native"
mkdir -p "$conf"
game="$(cat "$conf/game-folder" 2>/dev/null)"
if [ -z "$game" ] || [ ! -d "$game" ]; then
    game="$(osascript -e 'POSIX path of (choose folder with prompt "Choose the folder of Atlantis: The Lost Tales (with atlantis.exe)")')" || exit 0
    printf '%s' "$game" > "$conf/game-folder"
fi
if ! "$here/atlantis-native" "$game" 2> "$conf/last-error.txt"; then
    rm -f "$conf/game-folder"
    osascript -e "display alert \"Atlantis Native\" message \"$(tail -n 3 "$conf/last-error.txt" | tr '\"' "'")\"" >/dev/null
fi
