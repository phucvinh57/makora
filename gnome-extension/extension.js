/**
 * Makora's focus reporter.
 *
 * Wayland deliberately gives a client no way to ask which window is focused,
 * and GNOME closes the two loopholes that exist elsewhere: the
 * `org.gnome.Shell.Introspect` D-Bus API is allowlisted to
 * xdg-desktop-portal-gnome and answers everyone else with `Access denied`,
 * and `ext-foreign-toplevel-list-v1` — which Mutter does implement — carries
 * no focus state at all. So the only way for Makora to know what you're
 * working in is for something to run inside GNOME Shell and tell it. That's
 * this extension, and that's all it does.
 *
 * The bus name, object path, interface and protocol version below are
 * duplicated in `src-tauri/src/core/platform/linux/gnome.rs` — change one,
 * change the other. See README.md for the contract.
 */

import Gio from "gi://Gio";
import GLib from "gi://GLib";
import Shell from "gi://Shell";
import { Extension } from "resource:///org/gnome/shell/extensions/extension.js";

const BUS_NAME = "dev.phucvinh57.MakoraShell";
const OBJECT_PATH = "/dev/phucvinh57/MakoraShell";
const PROTOCOL_VERSION = 1;

/**
 * `present` is false when nothing is focused — most commonly because the
 * Activities overview is open. Makora treats that as "no change" and keeps
 * the running session open rather than ending it, so glancing at the
 * overview doesn't chop a session in two.
 */
const INTERFACE = `
<node>
  <interface name="dev.phucvinh57.MakoraShell.Focus1">
    <method name="GetFocusedWindow">
      <arg type="b" direction="out" name="present"/>
      <arg type="u" direction="out" name="pid"/>
      <arg type="s" direction="out" name="app_id"/>
      <arg type="s" direction="out" name="wm_class"/>
      <arg type="s" direction="out" name="title"/>
    </method>
    <signal name="FocusChanged">
      <arg type="b" name="present"/>
      <arg type="u" name="pid"/>
      <arg type="s" name="app_id"/>
      <arg type="s" name="wm_class"/>
      <arg type="s" name="title"/>
    </signal>
    <property name="Version" type="u" access="read"/>
  </interface>
</node>`;

class FocusReporter {
  constructor() {
    this._exported = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
    this._focusChangedId = 0;
    this._nameId = 0;
  }

  get Version() {
    return PROTOCOL_VERSION;
  }

  start() {
    // Export before claiming the name, so there is no instant where a
    // client can see the name and find nothing behind it.
    this._exported.export(Gio.DBus.session, OBJECT_PATH);
    this._nameId = Gio.bus_own_name(
      Gio.BusType.SESSION,
      BUS_NAME,
      Gio.BusNameOwnerFlags.NONE,
      null,
      null,
      null,
    );
    this._focusChangedId = global.display.connect("notify::focus-window", () =>
      this._onFocusChanged(),
    );
  }

  stop() {
    if (this._focusChangedId) {
      global.display.disconnect(this._focusChangedId);
      this._focusChangedId = 0;
    }
    if (this._nameId) {
      Gio.bus_unown_name(this._nameId);
      this._nameId = 0;
    }
    this._exported.unexport();
  }

  /**
   * Lets Makora ask what is focused the moment it connects. Without this
   * it would record nothing until the next time you switched apps —
   * which, if you start Makora and then keep working in the same window,
   * could be hours.
   */
  GetFocusedWindow() {
    return this._describe(global.display.focus_window);
  }

  _onFocusChanged() {
    const payload = this._describe(global.display.focus_window);
    this._exported.emit_signal(
      "FocusChanged",
      new GLib.Variant("(busss)", payload),
    );
  }

  /**
   * @param {Meta.Window|null} win the focused window, if any
   * @returns {[boolean, number, string, string, string]} the wire payload
   */
  _describe(win) {
    if (!win) return [false, 0, "", "", ""];

    // Makora keys sessions on the executable behind the window, which it
    // reads from /proc/<pid>/exe. `get_pid` returns -1 for windows the
    // compositor has no process for; 0 is the wire's "unknown", and
    // Makora falls back to the app id in that case.
    const pid = win.get_pid();
    const app = Shell.WindowTracker.get_default().get_window_app(win);

    return [
      true,
      pid > 0 ? pid : 0,
      app ? app.get_id() : "",
      win.get_wm_class() ?? "",
      win.get_title() ?? "",
    ];
  }
}

export default class MakoraFocusExtension extends Extension {
  enable() {
    this._reporter = new FocusReporter();
    this._reporter.start();
  }

  disable() {
    // Must fully tear down: `session-modes` is `user` only, so GNOME
    // disables this extension whenever the screen locks, and a leaked
    // signal handler or bus name would survive into the lock screen.
    this._reporter?.stop();
    this._reporter = null;
  }
}
