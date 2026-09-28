{
  description = "necromy-table — pixel-art hex board game on Bevy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # Libraries Bevy loads at runtime (dlopen): graphics, windowing, audio, input.
        runtimeLibs = with pkgs; [
          vulkan-loader
          libxkbcommon
          wayland
          alsa-lib
          udev
          libx11
          libxcursor
          libxi
          libxrandr
        ];
      in
      {
        # The dedicated server as one static (musl) binary: it runs on any
        # Linux host whatever its nixpkgs, e.g. the NixOS box in ../aerospace
        # (scripts/deploy-server.sh vendors it there). Only the Rust sources
        # go in; assets and art stay out.
        packages.necromy-server-static = pkgs.pkgsStatic.rustPlatform.buildRustPackage {
          pname = "necromy-server";
          version = "0.1.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./crates ./src ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "necromy-server" ];
          # The workspace's tests need sockets and a GPU-free Bevy build:
          # run them with `cargo test` in the dev shell instead.
          doCheck = false;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            pkg-config
            clang
            mold
          ];

          buildInputs = runtimeLibs;

          # Asset pipeline tools: scripts/gpt-image.sh, scripts/stable-audio.sh
          packages = with pkgs; [ curl jq imagemagick uv ffmpeg ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          # Native libraries for the pip wheels of scripts/stable-audio.sh (torch, soundfile).
          NECROMY_PYLIBS = pkgs.lib.makeLibraryPath (with pkgs; [ stdenv.cc.cc.lib zlib libsndfile ]);

          shellHook = ''
            if [ -f .env ]; then
              set -a; . ./.env; set +a
            fi
          '';
        };
      });
}
