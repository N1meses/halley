# Halley v0.8.0 — The Field Comes First

Halley v0.8.0 makes the **Field the starting point**: open applications, move them freely, arrange what is visible, and create a named cluster when a group of work needs one. Fresh configurations no longer begin with predefined clusters.

This release adds reversible window arrangements, clearer onboarding, screenshot previews, broader desktop integration, and a substantial focus, presentation, session, and portal reliability pass. **Lift now has its own repository and release track**, using Halley UI for shared components, layout, text, icons, and direct software drawing.

## Highlights

### Arrange without giving up the Field

`Mod+A` gathers visible windows into a monitor-local mosaic. An untouched arrangement toggles back to its saved geometry, including during animation. Moving, resizing, transferring, or closing a participating window ends that snapshot; the next press arranges the current windows afresh. Focus changes and stationary clicks preserve undo.

Clusters remain optional named contexts. An empty active cluster shows the configured close shortcut: press it once, release it, then press it again to delete the cluster and return to the Field. Escape cancels, and launchers, interactive overlays, or new members clear confirmation. Closing the final cluster member keeps focus inside that cluster, protecting windows elsewhere in the Field.

### A clearer first session and better everyday integration

The non-modal basics card introduces five operations and appears once per Halley version, including for existing configurations. Screenshot previews provide Copy and Open controls, while foreign-toplevel and workspace protocols let taskbars and docks work with windows and clusters. Native Wayland input methods use upstream text-input-v3 and input-method-v2 support.

### Independent ecosystem, shared UI

[Lift](https://github.com/saltnpepper97/halley-lift) is now released separately as `0.3.0`. Its launcher command and configuration stay familiar; install it separately or through the ecosystem package. [Halley UI](https://github.com/saltnpepper97/halley-ui) supplies shared text services and measured notification layout to the compositor, while Halley keeps its native GPU renderer, Wayland buffers, and required blur-capable Smithay revision.

## Added

- Reversible `Mod+A` Field arrangements with minimum-travel placement, size-constraint handling, animation, and temporary decay protection.
- Two-press deletion of an empty active cluster, with the actual configured shortcut, Escape cancellation, and protection against held-key confirmation.
- Optional startup cluster declarations with names, explicit members, layout, output, and launch attribution; empty `members []` is supported.
- A versioned basics introduction and a one-time notice explaining the first automatic collapse.
- Screenshot previews with Copy and Open controls. Hover preserves the preview, and copied PNG contents remain available after it closes.
- Foreign-toplevel window listing and controls for taskbars and docks, plus `ext-workspace-v1` cluster enumeration and atomic activation requests.
- Upstream native IME support through text-input-v3 and input-method-v2.
- Hardware cursor planes where supported, with reloadable `cursor.disable-hardware-cursor` for driver workarounds.
- Content-type, XDG toplevel-icon, and single-pixel-buffer protocol support.
- Directional Field window transfers between monitors, optional keyboard panning, and corresponding `halleyctl` commands.
- Grabbed-window edge panning through the current output's Field using `Mod+Shift+left-drag`.
- Custom opening and closing fragment shaders, matching wave examples, and documented fallback behavior.
- Signed notification offsets, independent titlebar text sizing, and a separate node-collapse animation duration.
- Bounded persistent autostart output logs and exit status.
- Shared Halley UI text shaping, glyph rasterization, and cached notification layout.

## Changed

- Fresh sessions start without predefined clusters. Fresh configurations use longer decay defaults; existing explicit values remain unchanged.
- Manual edits to arranged participants discard that output's restore snapshot instead of letting a later `Mod+A` overwrite newer placement.
- Lift moves out of the compositor workspace into its own repository and independent version track. The launcher remains `halley-lift`.
- The basics introduction is offered once per compositor version; reinstalling the same version preserves its dismissal.
- Configuration updates remain manual. `halleyctl config migrate` and automatic migration/backup code are removed.
- Smithay and smithay-drm-extras use the required unmodified upstream pin; the maintained vendor snapshot is removed. Blur and local animations use conservative full-output repaints.
- Client opacity affects client content and popups while compositor titlebars, borders, badges, and shadows stay opaque.
- Screenshot previews use Copy and Open controls without a close button.
- Compositor shortcuts remain available during exclusive layer-shell focus, subject to session locks and shortcut inhibitors.
- Documentation now presents the Field, nodes, retrieval tools, and optional clusters consistently. The website adds a searchable, versioned wiki with smooth sidebar expansion, and the README has refreshed demos.

## Fixed

- Keep focus on the cluster core after its final member closes, and prevent close actions from reaching hidden Field windows.
- Suppress the verified X11 attribute-lookup race for already-destroyed helper windows without hiding other X11, connection, or device failures.
- Preserve complete live window content, blur, masks, chrome, and presentation feedback through custom opening shaders; remove detached shadows during custom effects.
- Preserve client content at fixed endpoint scales during arrangements and always present the final live geometry frame.
- Keep size-constrained windows in feasible asymmetric mosaics and raise arrangement participants together without changing keyboard focus.
- Restore maximize/fullscreen handoffs, parked fullscreen retrieval, game focus-loss signaling, and state-only fullscreen exit completion.
- Restore XWayland dropdown placement and popup input regions, and keep client click-drags in the grabbed surface's coordinate space.
- Bridge clipboard and primary-selection transfers between Wayland and XWayland, and prevent temporary clipboard helpers from stealing cluster focus.
- Keep popup dismissal, layer-shell keyboard focus, and popup frame callbacks consistent; preserve compositor shortcuts on exclusive layers.
- Restore direct-session service activation and cleanup, refresh portal backends on login, and release DRM resources before GPU session access ends.
- Restore portal capture startup and keep blocking work off the D-Bus executor so capture and cancellation remain responsive.
- Bound IPC connections, request deadlines, capture resources, and portal sessions; retain request ownership and explicit authorization for privileged client capabilities.
- Isolate lock-screen input, safely reject pipelined lock surfaces, and protect accessibility keyboard monitoring and grabs with explicit authorization.
- Read gamma files on a bounded worker and defer composited DMA-BUF use until its readiness fences signal.
- Keep reversible zoom displacement for nodes and cluster cores, improve core label placement, and preserve monitor-local keyboard targeting and transfers.
- Accept signed input acceleration values and quoted speeds through upstream Rune parsing.
- Keep screenshot-preview cursor motion and overlay redraw demand local to the affected output.
- Fix optional-feature builds and retain startup GPU fallback on hybrid systems.

## Configuration notes

Existing configurations are not rewritten. Fresh defaults apply to newly generated files; add the arrangement, launcher, and transfer bindings manually if you want them, then run:

```console
halleyctl config verify
halleyctl reload
```

Screen locking disconnects native IME clients; reconnect or restart the input method after unlock. The upstream IME path replaces Halley's former patched lifecycle behavior. Slow secondary-monitor wake on AMD remains under investigation.

AUR packaging updates follow separately; the `v0.8.0` source tag identifies this release independently of when those packages are updated.

## Package versions

| Package | Version |
| --- | --- |
| Halley compositor | `0.8.0` |
| `halley-cli` / `halleyctl` | `0.8.0` |
| `halley-config` | `0.8.0` |
| `halley-core` | `0.7.0` |
| `halley-ipc` | `0.7.0` |
| `halley-api` | `0.4.0` |
| `halley-portal` | `0.2.1` |
| Lift | `0.3.0` |
| Halley UI | `0.1.2` |

Packages follow their own changes rather than sharing one version number. Halley itself is distributed from its source release; the publishable workspace crates are available through crates.io.

See the [0.8 wiki](https://saltnpepper97.github.io/halley-site/wiki/?version=0.8.0) for the current workflow and configuration reference, and the full [changelog](https://github.com/saltnpepper97/halley/blob/v0.8.0/CHANGELOG.md) for the complete change list.
