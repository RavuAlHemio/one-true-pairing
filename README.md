# one-true-pairing

Minimal [OTP](https://en.wikipedia.org/wiki/One-time_password) (specifically
[TOTP](https://datatracker.ietf.org/doc/html/rfc6238)) client named after the
[other kind](https://en.wikipedia.org/wiki/Shipping_%28fandom%29#Notation_and_terminology) of OTP.

Click on an icon in the notification bar, choose the account from a menu, and the OTP code is copied
into your clipboard.

## Architecture

The client:

1. queries OTP secrets via the D-Bus-based
   [freedesktop Secret Service API](https://specifications.freedesktop.org/secret-service-spec/latest/),
   implemented e.g. by [GNOME Keyring](https://gitlab.gnome.org/GNOME/gnome-keyring) or
   [KWallet](https://invent.kde.org/frameworks/kwallet).

2. offers a notification icon and menu using the D-Bus-based
   [KDE StatusNotifierItem API](https://invent.kde.org/frameworks/kstatusnotifieritem/-/blob/master/src/org.kde.StatusNotifierItem.xml)
   and
   [D-Bus Menu API](https://git.launchpad.net/ubuntu/+source/libdbusmenu/tree/libdbusmenu-glib/dbus-menu.xml),
   implemented not only by KDE but also by e.g. [Waybar](https://github.com/Alexays/Waybar).

3. when an OTP secret is chosen, provides it to the Wayland _selection_ (clipboard) using the
   [ext_data_control](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/tree/main/staging/ext-data-control)
   extension, supported e.g. by [Sway](https://github.com/swaywm/sway).

`one-true-pairing` does not depend on any UI framework and should work independently of your chosen
secrets provider or Wayland compositor, provided they support the aforementioned APIs.

Major dependencies are the `tokio` (for asynchronous I/O) and `zbus` (for D-Bus support) crates.
