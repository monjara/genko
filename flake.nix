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
        naersk-lib = pkgs.callPackage naersk { };
        nativeBuildInputs = with pkgs; [
          cmake
          llvmPackages.clang
          pkg-config
          rustPlatform.bindgenHook
        ];
        buildInputs = with pkgs; [
          alsa-lib
          fontconfig
          glib
          libGL
          libva
          libxkbcommon
          openssl
          pipewire
          sqlite
          vulkan-loader
          wayland
          xdg-desktop-portal
          libx11
          libxcursor
          libxi
          libxrandr
          libxcb
          zstd
        ];
        devTools = with pkgs; [
          cargo
          nixpkgs-fmt
          pre-commit
          rust-analyzer
          rustc
          rustfmt
          rustPackages.clippy
          vulkan-tools
        ];
        runtimeLibraryPath = pkgs.lib.makeLibraryPath buildInputs;
        package = naersk-lib.buildPackage {
          src = ./.;
          inherit nativeBuildInputs buildInputs;
        };
        devShell = pkgs.mkShell {
          packages = devTools;
          inherit nativeBuildInputs buildInputs;

          LD_LIBRARY_PATH = runtimeLibraryPath;
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        };
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
