#!/usr/bin/env sh
# A short tour of wbat64 from a POSIX shell.
# The UI is drawn on the terminal device, so command substitution only captures
# the result variables.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
wbat64=${WBAT64_EXE:-wbat64}

out=$("$wbat64" box "@$here/WBAT.TXT:signup" --emit sh)
rc=$?
if [ "$rc" -ge 100 ]; then
    echo "cancelled"
    exit 1
fi
eval "$out"
echo "Hello $name from $city (over 21: $WCB1, newsletter: $WCB2)"
