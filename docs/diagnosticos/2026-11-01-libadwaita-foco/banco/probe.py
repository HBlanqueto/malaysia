# Real libadwaita app for the focus-mode run: toggles fullscreen on a
# schedule and logs fullscreened / reveal-top-bars of its toolbar views.
import os, sys, time
import gi
gi.require_version('Gtk', '4.0'); gi.require_version('Adw', '1')
from gi.repository import Gtk, Adw, GLib

T0 = time.monotonic()
def log(*a): print(f"probe {time.monotonic()-T0:6.2f}s", *a, flush=True)

def on_activate(app):
    win = Adw.ApplicationWindow(application=app, default_width=800, default_height=500)
    split = Gtk.Box()
    side = Adw.ToolbarView(); side.add_top_bar(Adw.HeaderBar()); side.set_content(Gtk.Label(label="side"))
    main = Adw.ToolbarView(); main.add_top_bar(Adw.HeaderBar())
    inner = Adw.ToolbarView(); inner.add_top_bar(Gtk.SearchBar(search_mode_enabled=True)); inner.set_content(Gtk.Label(label="content"))
    main.set_content(inner)
    split.append(side); split.append(main)
    win.set_content(split)
    views = {'side': side, 'main': main, 'inner': inner}
    def state(*_):
        log("fullscreen=%s" % win.is_fullscreen(),
            " ".join(f"{k}={v.get_reveal_top_bars()}" for k, v in views.items()))
    win.connect('notify::fullscreened', state)
    for v in views.values(): v.connect('notify::reveal-top-bars', state)
    if os.environ.get('PROBE_APP_MANAGES'):
        # Like a viewer: hide the main header itself in fullscreen
        def own(*_):
            main.set_reveal_top_bars(not win.is_fullscreen())
        win.connect('notify::fullscreened', own)
    sched = [float(x) for x in os.environ.get('PROBE_SCHEDULE', '2,13,15').split(',')]
    GLib.timeout_add(int(sched[0]*1000), lambda: (log("-> fullscreen()"), win.fullscreen(), False)[-1])
    GLib.timeout_add(int(sched[1]*1000), lambda: (log("-> unfullscreen()"), win.unfullscreen(), False)[-1])
    GLib.timeout_add(int(sched[2]*1000), lambda: (state(), app.quit(), False)[-1])
    win.present(); state()

log("libadwaita:", sorted({l.split()[-1].split("/lib/")[0] for l in open("/proc/self/maps") if "libadwaita" in l}))
app = Adw.Application(application_id='org.malaysia.FocusProbe')
app.connect('activate', on_activate)
app.run([])
