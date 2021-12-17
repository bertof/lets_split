{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };


  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
      in
      rec {
        packages = flake-utils.lib.flattenTree {
          hello = pkgs.hello;
          sample = pkgs.gitAndTools;
        };
        defaultPackage = packages.hello;

        apps = {
          hello = flake-utils.lib.mkApp { drv = packages.hello; };
        };
        defaultApp = apps.hello;

        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            (rust-bin.stable.latest.default.override {
              targets = [ "x86_64-unknown-linux-gnu" "thumbv7em-none-eabihf" ];
            })
            bacon
            cargo-watch
            cargo-embed
            cargo-outdated
            stdenv.cc.cc.lib
            gcc-arm-embedded
            gdb-multitarget

            git
            cmake
            minicom
            openocd
            expect

            # bashInteractive
          ];

          depsBuildBuild = with pkgs; [ qemu ];

          LD_LIBRARY_PATH = "${pkgs.stdenv.cc.cc.lib}/lib";

          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.stdenv.cc.targetPrefix}cc";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = "qemu-aarch64";
        };
      }
    );

}
