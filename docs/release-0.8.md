# Halley 0.8 release preparation

Prepared on dev; merge validated preparation into main without creating the
Halley release tag. Website approval is still required before deployment or
Halley release. Keep the existing config-loaded notification.

## Independent versions

| Package | Version | Reason / status |
| --- | --- | --- |
| halley | 0.8.0 | Field workflow, integrations, and fixes; binary distributed from GitHub |
| halley-cli | 0.8.0 | New controls and removal of config migration; publish after approval |
| halley-config | 0.8.0 | New settings and removal of migration API; publish after approval |
| halley-core | 0.7.0 | Empty clusters and optional master; publish after approval |
| halley-api | 0.4.0 | New controls / IPC types; published prerequisite for Lift |
| halley-ipc | 0.7.0 | New controls and protocol hardening; published prerequisite for Lift |
| halley-portal | 0.2.1 | Capture, authorization, and lifecycle fixes; publish after approval |
| halley-lift | 0.3.0 | Standalone repository and Halley UI migration; release first |
| halley-ui | 0.1.2 | Composed buttons and software drawing optimizations; published prerequisite |
| rune-cfg | 0.7.1 | Existing signed-number release tag; published prerequisite |

Halley itself remains `publish = false`. The other workspace packages have no
Smithay dependency. Do not bump unchanged ecosystem packages to match Halley.

## Rendering constraint

Keep Smithay and smithay-drm-extras at revision
`79bbed5e1199090d787115614847a79c76607181`. This pin supplies the rendering interface
and blur support used by Halley. Do not change it to make a package publishable.
The UI crate owns text, layout, components, and software drawing; the compositor
owns GPU and Wayland integration.

## Final approval gate

1. Verify Lift's standalone GitHub release and crates.io package.
2. Review the local website: home, 0.8 news, current and archived wiki versions,
   configuration, Field workflow, install guide, IPC, and ecosystem links.
3. Obtain the user's approval before pushing the site's main branch: that push
   automatically deploys GitHub Pages. Update the release date if approval occurs
   on another day.
4. Deploy the approved site, then tag and publish the Halley 0.8 GitHub release
   following the existing release format. Do not tag during preparation.
5. Publish remaining crates in dependency order: config/core, CLI, then portal.
   Check actual registry prerequisites and use tested package contents.
6. Update AUR stable packages and the Lift source URL/tag for its new repository;
   update .SRCINFO together with each PKGBUILD. halley-full depends on package
   names and remains valid until these updates. Old immutable tags still work.
7. Make local and remote dev match main after release commits.

Compilation and frame comparisons do not establish fresh live-hardware
performance parity. Installation and restarting the compositor are separate
steps, outside this preparation request.
