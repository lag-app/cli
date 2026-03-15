{
  description = "Lag CLI development environment and package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        lib = pkgs.lib;
        workspace = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        webrtcTag = "webrtc-0001d84-2";
        webrtcPlatform =
          if system == "x86_64-linux" then
            "linux-x64"
          else if system == "aarch64-linux" then
            "linux-arm64"
          else if system == "x86_64-darwin" then
            "mac-x64"
          else if system == "aarch64-darwin" then
            "mac-arm64"
          else
            throw "Unsupported system for LiveKit WebRTC prebuilts: ${system}";
        webrtcPrebuilt = pkgs.fetchzip {
          url = "https://github.com/livekit/rust-sdks/releases/download/${webrtcTag}/webrtc-${webrtcPlatform}-release.zip";
          hash = "sha256-bGUC4UDxFeeRgXNs68Q1wUshbw6EBx7SsfmYRI62oS8=";
        };

        rustToolchain = with pkgs; [
          cargo
          clippy
          rustc
          rustfmt
        ];

        linuxNativeBuildInputs = with pkgs; [
          cmake
          pkg-config
          perl
        ];

        linuxBuildInputs = with pkgs; [
          alsa-lib
          glib
          libx11
          libopus
          libxi
          libxtst
          openssl
        ];

        nativeBuildInputs = rustToolchain ++ lib.optionals pkgs.stdenv.isLinux linuxNativeBuildInputs;

        buildInputs = lib.optionals pkgs.stdenv.isLinux linuxBuildInputs;

        package = pkgs.rustPlatform.buildRustPackage {
          pname = "lag-cli";
          version = workspace.workspace.package.version;
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "audiopus_sys-0.2.2" = "sha256-epzB54105Iihrfyj1HZNGSLOaihLw4rUZsT+rw/sXZs=";
            };
          };

          cargoBuildFlags = [
            "-p"
            "lag-cli"
          ];
          cargoTestFlags = [
            "-p"
            "lag-cli"
          ];

          nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux linuxNativeBuildInputs;
          buildInputs = buildInputs;

          LK_CUSTOM_WEBRTC = webrtcPrebuilt;

          meta = with lib; {
            description = "Terminal client for Lag voice chat";
            homepage = "https://github.com/lag-app/cli";
            license = licenses.mit;
            mainProgram = "lag";
            platforms = platforms.linux ++ platforms.darwin;
          };
        };
      in
      {
        packages = {
          default = package;
          lag-cli = package;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ package ];
          packages = nativeBuildInputs;

          LK_CUSTOM_WEBRTC = webrtcPrebuilt;

          shellHook = ''
            export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
          '';
        };
      }
    );
}
