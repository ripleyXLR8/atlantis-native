#!/bin/bash
# Atlantis Native, https://github.com/ripleyXLR8/atlantis-native (GPL-3.0)
# SDL2 front end based on Klaas Van den Broeck's (@Klaasvdb) R36S port,
# https://github.com/Klaasvdb/atlantis-native-handheld

XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}

if [ -d "/opt/system/Tools/PortMaster/" ]; then
  controlfolder="/opt/system/Tools/PortMaster"
elif [ -d "/opt/tools/PortMaster/" ]; then
  controlfolder="/opt/tools/PortMaster"
elif [ -d "$XDG_DATA_HOME/PortMaster/" ]; then
  controlfolder="$XDG_DATA_HOME/PortMaster"
else
  controlfolder="/roms/ports/PortMaster"
fi

source $controlfolder/control.txt
[ -f "${controlfolder}/mod_${CFW_NAME}.txt" ] && source "${controlfolder}/mod_${CFW_NAME}.txt"
get_controls

GAMEDIR=/$directory/ports/atlantisnative
cd "$GAMEDIR"

> "$GAMEDIR/log.txt" && exec > >(tee "$GAMEDIR/log.txt") 2>&1

if [ -z "$(ls gamedata | grep -v place-game-files-here)" ]; then
  pm_message "Copy your own Atlantis: The Lost Tales (CD or GOG.com) into ports/atlantisnative/gamedata."
  sleep 5
  pm_finish
  exit 1
fi

export SDL_GAMECONTROLLERCONFIG="$sdl_controllerconfig"
chmod +x ./atlantis-native-sdl.${DEVICE_ARCH}

# The game reads the pad itself (Select + Start quits): no gptokeyb.
pm_platform_helper "$GAMEDIR/atlantis-native-sdl.${DEVICE_ARCH}"
./atlantis-native-sdl.${DEVICE_ARCH} gamedata

pm_finish
