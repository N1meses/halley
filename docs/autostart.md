# Autostart commands and logs

`autostart.once` commands run once when a full TTY session starts, after
Halley's display sockets are ready. `autostart.on-reload` commands run after a
valid configuration reload. Nested Winit sessions do not run either group.

```rune
autostart:
  once "waybar"
  once "mako"
end
```

Each command gets a persistent output log under
`$XDG_STATE_HOME/halley/autostart`, falling back to
`~/.local/state/halley/autostart`. The filename starts with the executable name
and includes a stable hash of the full command line, so different arguments
get distinct files. Halley's session log prints the exact path at launch.

For example, to inspect Waybar failures after logging in:

```sh
tail -n 100 ~/.local/state/halley/autostart/waybar-*.log
```

Logs contain the launch time, command, selected display sockets, merged standard
output and error, and the shell's exit status. Logging is always enabled for
autostart commands; add a command's own debug flag when more detail is needed.
Normal keybind launches retain their existing output behavior.

Each command retains its current log and two older generations (`.log.1` and
`.log.2`), at most 1 MiB each. Reloads and repeated logins append to the same
bounded files. The log directory is private to the user (mode 0700), and files
use mode 0600. Logs persist across compositor restarts and reboots.

A small detached `halley-autolog` process captures each command's output. It
does not create another compositor and does not stop the command when Halley
exits. If log storage is unavailable, the command still starts. Autostart is a
convenience launcher: use the user service manager for services needing restart
policies or session-lifetime management.
