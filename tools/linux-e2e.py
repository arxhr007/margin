#!/usr/bin/env python3
"""End-to-end test of the Linux daemon, with no touchpad needed.

A fake multitouch touchpad (created through uinput) drives the real `margin` binary with finger
gestures, and the script reads back the key events that Margin's virtual keyboard emits.

    sudo python3 tools/linux-e2e.py target/release/margin [daemon-log-file]

Needs root (for /dev/uinput) and a kernel with uinput. It uses a private config folder in /tmp, so it
never touches your real settings. Note that a desktop session would also see the fake touchpad and move
its pointer, so run it in a VM, a container or WSL, or on a machine you are not using at the time.
"""
import fcntl, json, os, select, struct, subprocess, sys, time

BIN = sys.argv[1]
LOG = sys.argv[2] if len(sys.argv) > 2 else "/tmp/margin_e2e.log"
CFG = "/tmp/margin_e2e_cfg"
W, H = 1000, 500                      # raw touchpad coordinates

EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
BTN_LEFT, BTN_TOUCH, BTN_TOOL_FINGER = 0x110, 0x14A, 0x145
ABS_X, ABS_Y, ABS_MT_SLOT, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TRACKING_ID = 0, 1, 0x2F, 0x35, 0x36, 0x39
KEY = {"A": 56, "TAB": 15, "CTRL": 29, "SHIFT": 42, "VOLUP": 115, "VOLDOWN": 114, "PLAYPAUSE": 164, "NEXT": 163, "PREV": 165,
       "BRIGHTUP": 225, "BRIGHTDOWN": 224, "META": 125, "D": 32, "PAGEDOWN": 109, "MUTE": 113}
NAME = {v: k for k, v in KEY.items()}

UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT, UI_SET_PROPBIT = 0x40045564, 0x40045565, 0x40045567, 0x4004556E
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502


def ev(t, c, v):
    return struct.pack("llHHi", 0, 0, t, c, v)


class FakePad:
    def __init__(self):
        self.fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
        for e in (EV_KEY, EV_ABS, EV_SYN):
            fcntl.ioctl(self.fd, UI_SET_EVBIT, e)
        for k in (BTN_LEFT, BTN_TOUCH, BTN_TOOL_FINGER):
            fcntl.ioctl(self.fd, UI_SET_KEYBIT, k)
        for a in (ABS_X, ABS_Y, ABS_MT_SLOT, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TRACKING_ID):
            fcntl.ioctl(self.fd, UI_SET_ABSBIT, a)
        for p in (0, 2):  # INPUT_PROP_POINTER, INPUT_PROP_BUTTONPAD
            fcntl.ioctl(self.fd, UI_SET_PROPBIT, p)
        absmax, absmin = [0] * 64, [0] * 64
        absmax[ABS_X] = absmax[ABS_MT_POSITION_X] = W
        absmax[ABS_Y] = absmax[ABS_MT_POSITION_Y] = H
        absmax[ABS_MT_SLOT] = 4
        absmax[ABS_MT_TRACKING_ID] = 65535
        dev = struct.pack("80sHHHHi", b"Fake Touchpad", 3, 0x1234, 0x5678, 1, 0)
        dev += struct.pack("64i", *absmax) + struct.pack("64i", *absmin) + struct.pack("64i", *([0] * 64)) + struct.pack("64i", *([0] * 64))
        os.write(self.fd, dev)
        fcntl.ioctl(self.fd, UI_DEV_CREATE)
        self.next_id = 100

    def close(self):
        fcntl.ioctl(self.fd, UI_DEV_DESTROY)
        os.close(self.fd)

    def _w(self, *events):
        os.write(self.fd, b"".join(events))

    def down(self, x, y, slot=0):
        self.next_id += 1
        self._w(ev(EV_ABS, ABS_MT_SLOT, slot), ev(EV_ABS, ABS_MT_TRACKING_ID, self.next_id), ev(EV_ABS, ABS_MT_POSITION_X, int(x * W)),
                ev(EV_ABS, ABS_MT_POSITION_Y, int((1 - y) * H)), ev(EV_KEY, BTN_TOUCH, 1), ev(EV_SYN, 0, 0))

    def move(self, x, y, slot=0):
        self._w(ev(EV_ABS, ABS_MT_SLOT, slot), ev(EV_ABS, ABS_MT_POSITION_X, int(x * W)), ev(EV_ABS, ABS_MT_POSITION_Y, int((1 - y) * H)), ev(EV_SYN, 0, 0))

    def up(self, slot=0):
        self._w(ev(EV_ABS, ABS_MT_SLOT, slot), ev(EV_ABS, ABS_MT_TRACKING_ID, -1), ev(EV_KEY, BTN_TOUCH, 0), ev(EV_SYN, 0, 0))

    def click(self, down):
        self._w(ev(EV_KEY, BTN_LEFT, 1 if down else 0), ev(EV_SYN, 0, 0))

    def swipe(self, x0, y0, x1, y1, steps=24, delay=0.012):
        """Finger lands at (x0,y0) in 0..1 pad coordinates (y = 1 is the top) and slides to (x1,y1)."""
        self.down(x0, y0)
        for i in range(1, steps + 1):
            time.sleep(delay)
            self.move(x0 + (x1 - x0) * i / steps, y0 + (y1 - y0) * i / steps)
        time.sleep(0.03)
        self.up()


def event_node(name):
    block = ""
    for line in open("/proc/bus/input/devices"):
        if line.strip() == "":
            block = ""
            continue
        block += line
        if "Handlers=" in line and f'Name="{name}"' in block:
            for tok in line.split():
                if tok.startswith("event"):
                    return "/dev/input/" + tok
    return None


def wait_for(fn, timeout, what):
    end = time.time() + timeout
    while time.time() < end:
        r = fn()
        if r:
            return r
        time.sleep(0.1)
    raise SystemExit(f"timed out waiting for {what}")


def drain(fd, settle=0.6):
    """Collect key events from the daemon's keyboard until it has been quiet for `settle` seconds."""
    out, last = [], time.time()
    while time.time() - last < settle:
        r, _, _ = select.select([fd], [], [], 0.05)
        if not r:
            continue
        data = os.read(fd, 24 * 64)
        for i in range(0, len(data), 24):
            _, _, t, c, v = struct.unpack("llHHi", data[i:i + 24])
            if t == EV_KEY:
                out.append((NAME.get(c, str(c)), v))
                last = time.time()
    return out


def write_settings(mutate):
    path = f"{CFG}/margin/settings.json"
    s = json.load(open(path))
    mutate(s)
    json.dump(s, open(path, "w"), indent=2)
    time.sleep(1.6)  # the daemon checks the file once a second


results = []


def check(name, ok, detail=""):
    results.append(ok)
    print(("PASS  " if ok else "FAIL  ") + name + (f"   got: {detail!r}" if not ok else ""))


def presses(events, key):
    return sum(1 for k, v in events if k == key and v == 1)


def main():
    subprocess.run(["rm", "-rf", CFG])
    os.makedirs(CFG)
    pad = FakePad()
    env = dict(os.environ, XDG_CONFIG_HOME=CFG, XDG_RUNTIME_DIR=CFG)
    daemon = subprocess.Popen([BIN, "--diag"], env=env, stdout=open(LOG, "w"), stderr=subprocess.STDOUT)
    try:
        kb = wait_for(lambda: event_node("Margin virtual keyboard"), 8, "the daemon's virtual keyboard")
        wait_for(lambda: "touchpad: Fake Touchpad" in open(LOG).read(), 8, "the daemon to find the fake touchpad")
        fd = os.open(kb, os.O_RDONLY | os.O_NONBLOCK)
        time.sleep(0.5)

        # 1. left edge, slide up: volume up (default: left = volume)
        pad.swipe(0.01, 0.30, 0.01, 0.62)
        e = drain(fd)
        check("left edge slide up -> volume up", presses(e, "VOLUP") >= 2 and presses(e, "VOLDOWN") == 0, e)

        # 2. left edge, slide down: volume down
        pad.swipe(0.01, 0.70, 0.01, 0.38)
        e = drain(fd)
        check("left edge slide down -> volume down", presses(e, "VOLDOWN") >= 2 and presses(e, "VOLUP") == 0, e)

        # 3. top edge, slide right: Alt held, Tab pressed, Alt released after the finger lifts (default: top = Alt+Tab)
        pad.down(0.30, 0.995)
        for i in range(1, 25):
            time.sleep(0.012)
            pad.move(0.30 + 0.30 * i / 24, 0.995)
        time.sleep(0.4)
        mid = drain(fd, 0.3)
        pad.up()
        late = drain(fd)
        allev = mid + late
        check("top edge slide right -> Alt held + Tab", allev[:1] == [("A", 1)] and presses(allev, "TAB") >= 1, allev)
        check("Alt is released only when the finger lifts", ("A", 1) in mid and ("A", 0) not in mid and allev[-1:] == [("A", 0)], f"mid={mid} late={late}")

        # 4. a finger that lands inside the pad and drifts onto the edge is ordinary pointer use
        pad.swipe(0.30, 0.50, 0.01, 0.50, steps=10)
        pad.swipe(0.01, 0.55, 0.01, 0.90, steps=12) if False else None
        e = drain(fd)
        check("finger starting inside the pad triggers nothing", e == [], e)

        # 5. two fingers never trigger
        pad.down(0.01, 0.30, slot=0)
        pad.down(0.60, 0.30, slot=1)
        for i in range(1, 15):
            time.sleep(0.012)
            pad.move(0.01, 0.30 + 0.02 * i, slot=0)
        pad.up(0)
        pad.up(1)
        e = drain(fd)
        check("two-finger contact triggers nothing", e == [], e)

        # 6. corner press: bottom-right, one press -> play/pause after the multi-click wait (default: 3 actions there)
        pad.down(0.97, 0.03)
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.05)
        pad.click(False)
        time.sleep(0.05)
        pad.up()
        e = drain(fd, 1.0)
        check("bottom-right corner, 1 press -> play/pause", e == [("PLAYPAUSE", 1), ("PLAYPAUSE", 0)], e)

        # 7. corner, 2 quick presses -> next track
        pad.down(0.97, 0.03)
        for _ in range(2):
            time.sleep(0.06)
            pad.click(True)
            time.sleep(0.05)
            pad.click(False)
        time.sleep(0.05)
        pad.up()
        e = drain(fd, 1.0)
        check("bottom-right corner, 2 presses -> next track", e == [("NEXT", 1), ("NEXT", 0)], e)

        # 8. top-left corner, 1 press -> show desktop (Super+D)
        pad.down(0.03, 0.97)
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.05)
        pad.click(False)
        time.sleep(0.05)
        pad.up()
        e = drain(fd, 0.8)
        check("top-left corner -> Super+D", e == [("META", 1), ("D", 1), ("D", 0), ("META", 0)], e)

        # 9. settings change is picked up without a restart: left edge = brightness
        write_settings(lambda s: s["sections"]["left"].update(slide="brightness"))
        pad.swipe(0.01, 0.30, 0.01, 0.62)
        e = drain(fd)
        check("settings reload: left edge now brightness", presses(e, "BRIGHTUP") >= 1 and presses(e, "VOLUP") == 0, e)

        # 10. custom keys on the right edge: Ctrl+PageDown / Ctrl+PageUp, no hold
        def custom(s):
            s["sections"]["right"].update(slide="custom", forward="Ctrl+PageDown", back="Ctrl+PageUp", holdModifiers=False)
        write_settings(custom)
        pad.swipe(0.99, 0.30, 0.99, 0.75)
        e = drain(fd)
        ok = presses(e, "PAGEDOWN") >= 1 and presses(e, "CTRL") == presses(e, "PAGEDOWN") and all(v in (0, 1) for _, v in e)
        check("custom keys: Ctrl+PageDown per step", ok, e)

        # 11. press and hold a corner: bottom-left 1 press = next track, held = mute
        def hold(s):
            s["sections"]["bottomLeft"] = {"clicks": [{"do": "nextTrack"}], "longPress": {"do": "mute"}}
        write_settings(hold)
        pad.down(0.03, 0.03)
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.8)            # longer than the half-second hold
        mid = drain(fd, 0.2)
        pad.click(False)
        time.sleep(0.05)
        pad.up()
        late = drain(fd, 0.8)
        check("held corner -> its long-press action, while still held", mid == [("MUTE", 1), ("MUTE", 0)], mid)
        check("releasing a held corner runs no click action", late == [], late)
        pad.down(0.03, 0.03)
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.1)
        pad.click(False)
        time.sleep(0.05)
        pad.up()
        e = drain(fd, 0.8)
        check("short press on that corner -> its 1-press action", e == [("NEXT", 1), ("NEXT", 0)], e)

        # 12. a click waiting for more clicks isn't lost when another corner is pressed right after
        pad.down(0.97, 0.03)       # bottom-right: 1 press = play/pause, but it waits for a 2nd or 3rd
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.05)
        pad.click(False)
        time.sleep(0.03)
        pad.up()
        time.sleep(0.05)
        pad.down(0.03, 0.97)       # top-left within the wait: show desktop
        time.sleep(0.05)
        pad.click(True)
        time.sleep(0.05)
        pad.click(False)
        time.sleep(0.05)
        pad.up()
        e = drain(fd, 1.0)
        check("waiting corner click runs before the next corner's",
              e == [("PLAYPAUSE", 1), ("PLAYPAUSE", 0), ("META", 1), ("D", 1), ("D", 0), ("META", 0)], e)

        # 13. a broken settings.json is kept as a copy and the running settings stay (left = brightness)
        path = f"{CFG}/margin/settings.json"
        good = open(path).read()
        open(path, "w").write('{ "enabled": true, "speed": "fast" "oops" }')
        time.sleep(1.6)
        pad.swipe(0.01, 0.30, 0.01, 0.62)
        e = drain(fd)
        check("broken settings.json: keeps the settings it had", presses(e, "BRIGHTUP") >= 1 and presses(e, "VOLUP") == 0, e)
        check("broken settings.json: a copy is kept as settings.json.broken",
              os.path.exists(path + ".broken") and "oops" in open(path + ".broken").read())
        open(path, "w").write(good)
        time.sleep(1.6)

        # 14. a disabled Margin does nothing
        write_settings(lambda s: s.update(enabled=False))
        pad.swipe(0.01, 0.30, 0.01, 0.62)
        e = drain(fd)
        check("Margin off: no output", e == [], e)
    finally:
        daemon.terminate()
        try:
            daemon.wait(5)
        except Exception:
            daemon.kill()
        pad.close()
        print("--- daemon log ---")
        print("(full log: " + LOG + ")")

    passed = sum(results)
    print(f"\n{passed}/{len(results)} checks passed")
    sys.exit(0 if passed == len(results) else 1)


main()
