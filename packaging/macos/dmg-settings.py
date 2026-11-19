# dmgbuild settings for the D.E.V. Launcher disk image.
# dmgbuild -s packaging/macos/dmg-settings.py -D app=<.app> -D background=<.tiff> -D icon=<.icns> "D.E.V. Launcher" out.dmg
import os.path

app = defines["app"]
appname = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines.get("icon")

# The background is 660×420 (with a @2x layer); the icons sit in its two circles.
background = defines["background"]
window_rect = ((200, 140), (660, 420))
icon_locations = {appname: (170, 200), "Applications": (490, 200)}
icon_size = 112
text_size = 13
label_pos = "bottom"

default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False
arrange_by = None
