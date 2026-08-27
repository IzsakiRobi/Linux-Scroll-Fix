private const string APP_ID = "io.github.izsakirobi.LinuxScrollFix";
private const string HELPER = "/usr/local/libexec/linux-scroll-fixctl";
private const string PKEXEC = "/usr/bin/pkexec";

private class LinuxScrollFixWindow : Adw.ApplicationWindow {
    private Adw.SwitchRow service_row;
    private Adw.ComboRow profile_row;
    private Adw.ActionRow speed_row;
    private Adw.ComboRow direction_row;
    private Gtk.Scale speed_scale;
    private Adw.ToastOverlay toast_overlay;
    private Adw.Toast? status_toast;
    private bool updating;
    private bool busy;
    private uint speed_timeout_id;

    public LinuxScrollFixWindow (Gtk.Application application) {
        Object (
            application: application,
            title: "Linux Scroll Fix",
            default_width: 520,
            default_height: 430
        );

        var toolbar_view = new Adw.ToolbarView ();
        var header_bar = new Adw.HeaderBar ();
        toolbar_view.add_top_bar (header_bar);

        toast_overlay = new Adw.ToastOverlay ();
        var page = new Adw.PreferencesPage ();
        toast_overlay.child = page;
        toolbar_view.content = toast_overlay;
        content = toolbar_view;

        var general_group = new Adw.PreferencesGroup ();
        general_group.title = "General";
        page.add (general_group);

        service_row = new Adw.SwitchRow ();
        service_row.title = "Smooth Scrolling";
        service_row.subtitle = "Checking service status…";
        general_group.add (service_row);

        var profiles = new Gtk.StringList (null);
        profiles.append ("Precise");
        profiles.append ("Balanced");
        profiles.append ("Rapid");
        profiles.append ("Custom");
        profile_row = new Adw.ComboRow ();
        profile_row.title = "Profile";
        profile_row.subtitle = "Fine control with a gentle top speed";
        profile_row.model = profiles;
        profile_row.selected = 0;
        general_group.add (profile_row);

        speed_row = new Adw.ActionRow ();
        speed_row.title = "Scroll Speed";
        speed_row.subtitle = "Precise calibration";
        var speed_box = new Gtk.Box (Gtk.Orientation.VERTICAL, 0);
        speed_box.width_request = 255;
        speed_box.margin_top = 8;
        speed_box.margin_bottom = 8;
        speed_scale = new Gtk.Scale.with_range (
            Gtk.Orientation.HORIZONTAL,
            0.0,
            8.0,
            1.0
        );
        speed_scale.draw_value = false;
        speed_scale.set_round_digits (0);
        speed_scale.set_value (3.0);
        for (int level = 0; level <= 8; level++) {
            speed_scale.add_mark (level, Gtk.PositionType.BOTTOM, null);
        }
        speed_box.append (speed_scale);
        var speed_labels = new Gtk.Box (Gtk.Orientation.HORIZONTAL, 0);
        var slow_label = new Gtk.Label ("Slow");
        slow_label.add_css_class ("dim-label");
        slow_label.halign = Gtk.Align.START;
        slow_label.hexpand = true;
        var fast_label = new Gtk.Label ("Fast");
        fast_label.add_css_class ("dim-label");
        fast_label.halign = Gtk.Align.END;
        speed_labels.append (slow_label);
        speed_labels.append (fast_label);
        speed_box.append (speed_labels);
        speed_row.add_suffix (speed_box);
        speed_row.activatable_widget = speed_scale;
        general_group.add (speed_row);

        var directions = new Gtk.StringList (null);
        directions.append ("Traditional");
        directions.append ("Natural");
        direction_row = new Adw.ComboRow ();
        direction_row.title = "Scroll Direction";
        direction_row.model = directions;
        general_group.add (direction_row);

        service_row.notify["active"].connect (() => {
            if (!updating && !busy) {
                change_service.begin (service_row.active);
            }
        });
        profile_row.notify["selected"].connect (() => {
            if (!updating && !busy && profile_row.selected < 3) {
                change_profile.begin (profile_row.selected);
            } else if (!updating && !busy && profile_row.selected == 3) {
                profile_row.subtitle = "Use the speed slider below";
            }
        });
        speed_scale.value_changed.connect (() => {
            if (!updating && !busy) {
                schedule_speed_change ((uint) Math.round (speed_scale.get_value ()));
            }
        });
        direction_row.notify["selected"].connect (() => {
            if (!updating && !busy) {
                change_direction.begin (direction_row.selected);
            }
        });

        refresh_state.begin ();
    }

    private async void refresh_state () {
        set_busy (true);
        try {
            string output = yield run_command ({ HELPER, "status" });
            bool active = read_state (output, "active") == "true";
            bool enabled = read_state (output, "enabled") == "true";
            string profile = read_state (output, "profile");
            string speed = read_state (output, "speed");
            string direction = read_state (output, "direction");

            updating = true;
            service_row.active = active || enabled;
            switch (profile) {
                case "precise":
                    profile_row.selected = 0;
                    profile_row.subtitle = "Fine control with a gentle top speed";
                    break;
                case "balanced":
                    profile_row.selected = 1;
                    profile_row.subtitle = "Faster everyday scrolling";
                    break;
                case "rapid":
                    profile_row.selected = 2;
                    profile_row.subtitle = "Maximum speed for long pages";
                    break;
                default:
                    profile_row.selected = 3;
                    profile_row.subtitle = "Custom speed";
                    break;
            }
            uint speed_level = 3;
            if (speed != "unknown") {
                int parsed_speed = int.parse (speed);
                if (parsed_speed >= 0 && parsed_speed <= 8) {
                    speed_level = (uint) parsed_speed;
                }
            }
            speed_scale.set_value (speed_level);
            update_speed_subtitle (speed_level, profile);
            direction_row.selected = direction == "natural" ? 1 : 0;
            updating = false;

            if (active && enabled) {
                service_row.subtitle = "Running and starts automatically";
            } else if (active) {
                service_row.subtitle = "Running until the next restart";
            } else if (enabled) {
                service_row.subtitle = "Enabled, but not currently running";
            } else {
                service_row.subtitle = "Stopped";
            }
        } catch (Error error) {
            updating = true;
            service_row.active = false;
            updating = false;
            service_row.subtitle = "Service status is unavailable";
            show_error (error.message);
        }
        set_busy (false);
    }

    private async void change_service (bool enable) {
        set_busy (true);
        service_row.subtitle = enable ? "Starting…" : "Stopping…";
        try {
            yield run_command ({ PKEXEC, HELPER, enable ? "enable" : "disable" });
        } catch (Error error) {
            show_error (error.message);
        }
        yield refresh_state ();
    }

    private async void change_direction (uint selected) {
        set_busy (true);
        string direction = selected == 1 ? "natural" : "traditional";
        try {
            yield run_command ({ PKEXEC, HELPER, "set-direction", direction });
            show_status ("Scroll direction updated");
        } catch (Error error) {
            show_error (error.message);
        }
        yield refresh_state ();
    }

    private async void change_profile (uint selected) {
        cancel_speed_timeout ();
        set_busy (true);
        string profile;
        switch (selected) {
            case 0:
                profile = "precise";
                break;
            case 1:
                profile = "balanced";
                break;
            case 2:
                profile = "rapid";
                break;
            default:
                yield refresh_state ();
                return;
        }
        try {
            yield run_command ({ PKEXEC, HELPER, "set-profile", profile });
            show_status ("Profile updated");
        } catch (Error error) {
            show_error (error.message);
        }
        yield refresh_state ();
    }

    private void schedule_speed_change (uint level) {
        cancel_speed_timeout ();
        updating = true;
        profile_row.selected = 3;
        profile_row.subtitle = "Custom speed";
        update_speed_subtitle (level, "custom");
        updating = false;
        speed_timeout_id = Timeout.add (450, () => {
            speed_timeout_id = 0;
            change_speed.begin (level);
            return Source.REMOVE;
        });
    }

    private void cancel_speed_timeout () {
        if (speed_timeout_id != 0) {
            Source.remove (speed_timeout_id);
            speed_timeout_id = 0;
        }
    }

    private async void change_speed (uint level) {
        set_busy (true);
        try {
            yield run_command ({ PKEXEC, HELPER, "set-speed", level.to_string () });
            show_status ("Scroll speed updated");
        } catch (Error error) {
            show_error (error.message);
        }
        yield refresh_state ();
    }

    private void update_speed_subtitle (uint level, string profile) {
        switch (profile) {
            case "precise":
                speed_row.subtitle = "Precise calibration";
                break;
            case "balanced":
                speed_row.subtitle = "Balanced calibration";
                break;
            case "rapid":
                speed_row.subtitle = "Rapid calibration";
                break;
            default:
                speed_row.subtitle = "Custom level %u of 8".printf (level);
                break;
        }
    }

    private async string run_command (string[] arguments) throws Error {
        var process = new Subprocess.newv (
            arguments,
            SubprocessFlags.STDOUT_PIPE | SubprocessFlags.STDERR_PIPE
        );
        string? stdout_text;
        string? stderr_text;
        yield process.communicate_utf8_async (
            null,
            null,
            out stdout_text,
            out stderr_text
        );
        if (!process.get_successful ()) {
            string details = (stderr_text ?? "").strip ();
            if (details == "") {
                details = "The requested operation was not completed.";
            }
            throw new IOError.FAILED (details);
        }
        return stdout_text ?? "";
    }

    private string read_state (string output, string key) {
        string prefix = key + "=";
        foreach (string line in output.split ("\n")) {
            if (line.has_prefix (prefix)) {
                return line.substring (prefix.length).strip ();
            }
        }
        return "";
    }

    private void set_busy (bool busy) {
        this.busy = busy;
    }

    private void show_status (string message) {
        if (status_toast == null) {
            var toast = new Adw.Toast (message);
            status_toast = toast;
            toast.dismissed.connect (() => {
                if (status_toast == toast) {
                    status_toast = null;
                }
            });
        } else {
            status_toast.title = message;
        }
        toast_overlay.add_toast (status_toast);
    }

    private void show_error (string message) {
        if (status_toast != null) {
            status_toast.dismiss ();
            status_toast = null;
        }
        var toast = new Adw.Toast (message);
        toast.priority = Adw.ToastPriority.HIGH;
        toast_overlay.add_toast (toast);
    }
}

private class LinuxScrollFixApplication : Adw.Application {
    public LinuxScrollFixApplication () {
        Object (
            application_id: APP_ID,
            flags: ApplicationFlags.DEFAULT_FLAGS
        );
    }

    protected override void activate () {
        var window = active_window as LinuxScrollFixWindow;
        if (window == null) {
            window = new LinuxScrollFixWindow (this);
        }
        window.present ();
    }
}

int main (string[] args) {
    return new LinuxScrollFixApplication ().run (args);
}
