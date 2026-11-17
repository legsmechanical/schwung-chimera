#!/bin/bash
# Install CHIMERA to a Move (stock Schwung tree). MOVE_HOST overrides the target
# (default ableton@move.local).
#
# Never restarts anything: a module id the host has not loaded since its last
# start picks up the new dsp.so on its next load; one that IS loaded keeps the
# old code mapped until the host restarts (the host never unloads a synth
# library), and that restart is the user's to do.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
ID="chimera"
MOVE_HOST="${MOVE_HOST:-ableton@move.local}"
DEST="/data/UserData/schwung/modules/sound_generators/$ID"

cd "$REPO_ROOT"
[ -d "dist/$ID" ] || { echo "Error: dist/$ID not found. Run ./scripts/build.sh first."; exit 1; }

echo "=== Installing CHIMERA -> $MOVE_HOST:$DEST ==="
# (no user folders to keep)
# Copy to a temp name, then rename: writing over a dlopen'd dsp.so in place
# corrupts the running mapping; the rename keeps it on its old inode.
( cd "dist/$ID" && find . -type f ) | while read -r rel; do
    rel="${rel#./}"
    ssh -n "$MOVE_HOST" "mkdir -p \"\$(dirname '$DEST/$rel')\""
    scp -q "dist/$ID/$rel" "$MOVE_HOST:$DEST/$rel.new"
    ssh -n "$MOVE_HOST" "mv -f '$DEST/$rel.new' '$DEST/$rel' && chmod a+rw '$DEST/$rel'"
done

echo "Verifying..."
fail=0
while read -r rel; do
    rel="${rel#./}"
    want="$(md5 -q "dist/$ID/$rel" 2>/dev/null || md5sum "dist/$ID/$rel" | cut -d' ' -f1)"
    got="$(ssh -n "$MOVE_HOST" "md5sum '$DEST/$rel' | cut -d' ' -f1")"
    if [ "$want" = "$got" ]; then echo "  ok  $rel"; else echo "  MISMATCH  $rel"; fail=1; fi
done < <(cd "dist/$ID" && find . -type f)
[ "$fail" = 0 ] || { echo "Install verification FAILED"; exit 1; }
echo "=== Installed. If CHIMERA was already loaded in this host session, restart the host to load the new code. ==="
