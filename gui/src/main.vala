private const string APP_ID = "io.github.izsakirobi.LinuxScrollFix";
private const string HELPER = "/usr/local/libexec/linux-scroll-fixctl";
private const string PKEXEC = "/usr/bin/pkexec";

private class LinuxScrollFixWindow : Adw.ApplicationWindow {
    private Adw.SwitchRow service_row;
    private Adw.ComboRow profile_row;
    private Adw.ComboRow direction_row;
    private Adw.ToastOverlay toast_overlay;
    private bool updating;

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
        profile_row = new Adw.ComboRow ();
        profile_row.title = "Profile";
        profile_row.subtitle = "More profiles can be added later";
        profile_row.model = profiles;
        profile_row.selected = 0;
        general_group.add (profile_row);

        var directions = new Gtk.StringList (null);
        directions.append ("Traditional");
        directions.append ("Natural");
        direction_row = new Adw.ComboRow ();
        direction_row.title = "Scroll Direction";
        direction_row.model = directions;
        general_group.add (direction_row);

        service_row.notify["active"].connect (() => {
            if (!updating) {
                change_service.begin (service_row.active);
            }
        });
        direction_row.notify["selected"].connect (() => {
            if (!updating) {
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
            string direction = read_state (output, "direction");

            updating = true;
            service_row.active = active || enabled;
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
            toast_overlay.add_toast (new Adw.Toast ("Scroll direction updated"));
        } catch (Error error) {
            show_error (error.message);
        }
        yield refresh_state ();
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
        service_row.sensitive = !busy;
        profile_row.sensitive = !busy;
        direction_row.sensitive = !busy;
    }

    private void show_error (string message) {
        toast_overlay.add_toast (new Adw.Toast (message));
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
