{
  description = "necromy-table — pixel-art hex board game on Bevy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Only for the Windows shell: nixpkgs builds `rustc` with the host
    # standard library alone, so `--target x86_64-pc-windows-msvc` has no
    # `std`. rust-overlay hands out upstream binaries with every target.
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          # Adds `rust-bin`; `rustc`/`cargo` stay as nixpkgs built them.
          overlays = [ rust-overlay.overlays.default ];
        };

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

        # Cross-builds the Windows client (scripts/build-windows.sh). Apart
        # from `default`: another rustc, and its own CARGO_TARGET_DIR, so the
        # two toolchains do not keep invalidating each other's build scripts.
        devShells.windows = pkgs.mkShell {
          nativeBuildInputs = [
            # The same version as nixpkgs' rustc in the default shell.
            (pkgs.rust-bin.stable.${pkgs.rustc.version}.minimal.override {
              targets = [ "x86_64-pc-windows-msvc" ];
            })
            # Fetches the MSVC CRT and Windows SDK (~1 GB, once, into
            # XWIN_CACHE_DIR) and drives clang-cl/lld-link with them.
            pkgs.cargo-xwin
            # What cargo-xwin calls by name: clang-cl for the C in Bevy's
            # tree, lld-link, and llvm-lib (without it a build script fails
            # late with a message that never mentions Windows).
            pkgs.llvmPackages.clang-unwrapped
            pkgs.lld
            pkgs.llvmPackages.llvm
            pkgs.mold # the host linker flag in .cargo/config.toml
            pkgs.curl
            pkgs.zip
            pkgs.unzip
          ];

          CARGO_TARGET_DIR = "target-windows";
          # Build scripts and proc macros still link for Linux, and
          # .cargo/config.toml names "clang" for that, which here would be
          # the unwrapped one (no libc paths). Point at the wrapped one.
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.clang}/bin/clang";
          # Link the C runtime statically: otherwise the exe needs
          # VCRUNTIME140.dll from the Visual C++ Redistributable. Scoped to
          # the target, since build scripts and proc macros build for Linux.
          CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static";
          # The redistributable-CRT licence; cargo-xwin asks otherwise.
          XWIN_ACCEPT_LICENSE = "1";

          shellHook = ''
            export XWIN_CACHE_DIR="''${XWIN_CACHE_DIR:-$PWD/target-windows/.xwin}"
          '';
        };
      });
}
