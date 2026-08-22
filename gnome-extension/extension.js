import Gio from "gi://Gio";
import GLib from "gi://GLib";
import Shell from "gi://Shell";
import { Extension } from "resource:///org/gnome/shell/extensions/extension.js";

const BUS_NAME = "dev.phucvinh57.MakoraShell";
const OBJECT_PATH = "/dev/phucvinh57/MakoraShell";
const PROTOCOL_VERSION = 1;

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

  _describe(win) {
    if (!win) return [false, 0, "", "", ""];

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
    this._reporter?.stop();
    this._reporter = null;
  }
}
