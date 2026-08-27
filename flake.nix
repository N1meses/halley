# This flake is community maintained.
{
  description = "Halley: a spatial Wayland compositor built around infinite workspace navigation.";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    {
      self,
      nixpkgs,
    }:
    let
      inherit (nixpkgs) lib;

      revision = self.shortRev or self.dirtyShortRev or "unknown";

      halley-package =
        {
          lib,
          rustPlatform,
          pkg-config,
          libGL,
          libinput,
          libxkbcommon,
          libgbm,
          seatd,
          wayland,
          pipewire,
          dbus,
          vulkan-loader,
          systemd,
          eudev,
          libdisplay-info_0_3,
          withSystemd ? true,
        }:
        rustPlatform.buildRustPackage {
          pname = "halley";
          version = revision;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./assets
              ./halley-api
              ./halley-cli
              ./halley-config
              ./halley-core
              ./halley-ipc
              ./halley-lift
              ./halley-portal
              ./packaging
              ./examples
              ./src
              ./Cargo.toml
              ./Cargo.lock
            ];
          };

          cargoLock = {
            allowBuiltinFetchGit = true;
            lockFile = ./Cargo.lock;
          };

          # 0.6.0 made the root a package *and* the workspace root, so a bare
          # cargo build no longer picks up halleyctl, halley-lift, or the portal.
          cargoBuildFlags = [ "--workspace" ];

          strictDeps = true;

          nativeBuildInputs = [
            pkg-config
            rustPlatform.bindgenHook
          ];

          buildInputs = [
            wayland
            libxkbcommon
            libinput
            libgbm
            libGL
            seatd
            pipewire
            dbus
            libdisplay-info_0_3
          ]
          ++ [ (if withSystemd then systemd else eudev) ];

          appendRunpaths = [ "${lib.getLib vulkan-loader}/lib" ];

          doCheck = false;

          postInstall = ''
            # session launcher script (patchShebangs fixes its #!/bin/sh)
            patchShebangs packaging/wayland-sessions/halley-session
            substituteInPlace packaging/wayland-sessions/halley-session \
              --replace-fail ':-/usr/bin/halley}' ":-$out/bin/halley}"
            install -Dm755 packaging/wayland-sessions/halley-session -t $out/bin

            # greeter session entry (Exec=/TryExec= point at halley-session)
            substituteInPlace packaging/wayland-sessions/halley.desktop \
              --replace-fail '=halley-session' "=$out/bin/halley-session"
            install -Dm644 packaging/wayland-sessions/halley.desktop -t $out/share/wayland-sessions

            # portal interface declaration (no path inside — install as-is)
            install -Dm644 packaging/xdg-desktop-portal/portals/halley.portal \
              -t $out/share/xdg-desktop-portal/portals

            # portal backend preference (ScreenCast/Screenshot -> halley)
            install -Dm644 packaging/xdg-desktop-portal/halley-portals.conf \
              -t $out/share/xdg-desktop-portal

            # D-Bus activation for the screencast/screenshot portal binary
            substituteInPlace packaging/dbus-1/services/org.freedesktop.impl.portal.desktop.halley.service \
              --replace-fail /usr/bin/xdg-desktop-portal-halley $out/bin/xdg-desktop-portal-halley
            install -Dm644 packaging/dbus-1/services/org.freedesktop.impl.portal.desktop.halley.service \
              -t $out/share/dbus-1/services
          '';

          env = {
            RUSTFLAGS = toString (
              map (arg: "-C link-arg=" + arg) [
                "-Wl,--push-state,--no-as-needed"
                "-lEGL"
                "-lwayland-client"
                "-Wl,--pop-state"
              ]
            );
          };

          passthru.providedSessions = [ "halley" ];

          meta = {
            description = "Spatial Wayland compositor built around infinite workspace navigation";
            homepage = "https://github.com/saltnpepper97/halley";
            license = lib.licenses.gpl3Only;
            mainProgram = "halley";
            platforms = lib.platforms.linux;
          };
        };

      systems = lib.intersectLists lib.systems.flakeExposed lib.platforms.linux;
      forAllSystems = lib.genAttrs systems;
      nixpkgsFor = forAllSystems (system: nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        system:
        let
          halley = nixpkgsFor.${system}.callPackage halley-package { };
        in
        {
          inherit halley;
          default = halley;
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
        in
        {
          halley-tests = self.packages.${system}.halley.overrideAttrs (previous: {
            pname = "halley-tests";

            doCheck = true;

            cargoTestFlags = [ "--workspace" ];

            nativeCheckInputs = (previous.nativeCheckInputs or [ ]) ++ [ pkgs.dejavu_fonts ];
            FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; };
          });
        }
      );

      overlays.default = final: _prev: {
        halley = final.callPackage halley-package { };
      };

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          inherit (self.packages.${system}) halley;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ halley ];
            packages = [
              pkgs.rustc
              pkgs.cargo
              pkgs.clippy
              pkgs.rust-analyzer
              pkgs.rustfmt
            ];
            env.RUSTFLAGS = halley.RUSTFLAGS;
          };
        }
      );

      formatter = forAllSystems (system: nixpkgsFor.${system}.nixfmt);
    };
}
