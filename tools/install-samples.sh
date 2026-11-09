#!/bin/sh
# Install the bundled CC0 sample pads into a personal VFS and SFX mapping.
#
#   tools/install-samples.sh
#
# Override FUNGOS_VFS_ROOT, FUNGOS_APPDATA_ROOT, FUNGOS_COMMAND_ROOT, or
# SOIL_COMMAND when the personal instance lives elsewhere.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
samples="$root/spores/sfx/assets/samples"
vfs=${FUNGOS_VFS_ROOT:-$HOME/.local/share/fungos/vfs}
appdata=${FUNGOS_APPDATA_ROOT:-$HOME/.local/share/fungos/appdata}
commands=${FUNGOS_COMMAND_ROOT:-$HOME/.local/share/fungos/commands}
soil=${SOIL_COMMAND:-}
if [ -z "$soil" ]; then
  if command -v soil-command >/dev/null 2>&1; then soil=$(command -v soil-command); else
    for candidate in "$root/../soil/target/debug/soil-command" /tmp/sfx-target/debug/soil-command; do
      if [ -x "$candidate" ]; then soil=$candidate; break; fi
    done
  fi
fi
if [ -z "$soil" ]; then echo "install-samples: set SOIL_COMMAND" >&2; exit 2; fi

install_one() {
  file=$1; pad=$2; label=$3
  fungos-vfs --root "$vfs" put "$samples/$file" "sfx/samples/$file" \
    --media-type audio/mpeg >/dev/null
  "$soil" --root "$commands" --vfs-root "$vfs" --appdata-root "$appdata" \
    --audio run sfx sample add "$pad" "sfx/samples/$file" --label "$label" \
    < /dev/null >/dev/null 2>&1 || true
  echo "sample $pad -> sfx/samples/$file"
}

install_one confirm.mp3 confirm Confirm
install_one success.mp3 success Success
install_one click.mp3 click Click
install_one drop.mp3 whoosh Whoosh
install_one error.mp3 error Error
install_one pluck.mp3 pluck Pluck
install_one bong.mp3 bong Bong
install_one glass.mp3 glass Glass

echo "installed into $vfs and $appdata"
