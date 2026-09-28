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
