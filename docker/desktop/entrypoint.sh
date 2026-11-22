#!/bin/sh
# Start a virtual X display, a full-screen window manager, and a VNC server that noVNC serves to a browser.
# Then run the command given to the container, so the container stops when Elaris exits.
set -eu

export DISPLAY=:99
size=${ELARIS_DESKTOP_SIZE:-1600x1000}

Xvfb "$DISPLAY" -screen 0 "${size}x24" -nolisten tcp >/tmp/xvfb.log 2>&1 &

# Wait for the X server before starting clients that connect to it.
attempts=0
until xdpyinfo >/dev/null 2>&1; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 100 ]; then
        echo "elaris-desktop: the X server did not start" >&2
        cat /tmp/xvfb.log >&2
        exit 1
    fi
    sleep 0.1
done

openbox --config-file /etc/elaris-desktop/openbox.xml >/tmp/openbox.log 2>&1 &
x11vnc -display "$DISPLAY" -forever -shared -nopw -quiet -rfbport 5900 >/tmp/x11vnc.log 2>&1 &
websockify --web /usr/share/novnc 6080 localhost:5900 >/tmp/websockify.log 2>&1 &

echo "elaris-desktop: open http://127.0.0.1:6080/vnc.html?autoconnect=1&resize=scale" >&2
exec "$@"
