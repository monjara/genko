{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, utils, naersk, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        naersk-lib = pkgs.callPackage naersk { };
        nativeBuildInputs = with pkgs; [
          cmake
          llvmPackages.clang
          pkg-config
          rustPlatform.bindgenHook
        ];
        commonBuildInputs = with pkgs; [
          openssl
          sqlite
          zstd
        ];
        linuxBuildInputs = with pkgs; [
          alsa-lib
          fontconfig
          glib
          libGL
          libva
          libxkbcommon
          libx11
          libxcb
          libxcursor
          libxi
          libxrandr
          pipewire
          vulkan-loader
          wayland
          xdg-desktop-portal
        ];
        darwinBuildInputs = with pkgs; [
          apple-sdk
          libiconv
        ];
        buildInputs =
          commonBuildInputs
          ++ lib.optionals pkgs.stdenv.isLinux linuxBuildInputs
          ++ lib.optionals pkgs.stdenv.isDarwin darwinBuildInputs;
        devTools = with pkgs; [
          cargo
          cargo-make
          cargo-watch
          nixpkgs-fmt
          pre-commit
          rust-analyzer
          rustPackages.clippy
          rustc
          rustfmt
        ] ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          vulkan-tools
        ]);
        runtimeLibraryPath = pkgs.lib.makeLibraryPath buildInputs;
        package = naersk-lib.buildPackage {
          src = ./.;
          inherit nativeBuildInputs buildInputs;
        };
        devShell = pkgs.mkShell ({
          packages = devTools;
          inherit nativeBuildInputs buildInputs;

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        } // lib.optionalAttrs pkgs.stdenv.isLinux {
          LD_LIBRARY_PATH = runtimeLibraryPath;
        });
      in
      {
        packages.default = package;
        apps.default = utils.lib.mkApp { drv = package; };
        devShells.default = devShell;
        checks.default = package;
        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
