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
        minBuildInputs = with pkgs; [
          gcc-arm-embedded
          (rust-bin.stable.latest.default.override {
            targets = [ "x86_64-unknown-linux-gnu" "thumbv7em-none-eabihf" ];
          })
          stdenv.cc.cc.lib
        ];
      in
      rec {
        packages = flake-utils.lib.flattenTree {
          upload_usb = pkgs.writeShellScriptBin "upload_usb" ''
            export PATH="${pkgs.lib.makeBinPath (minBuildInputs ++ [pkgs.dfu-util])}":$PATH
            cargo build --release --bin ''${1:-split}
            arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/split split.bin
            sudo dfu-util -a 0 -s 0x8000000 -RD split.bin
          '';
        };


        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            bacon
            cargo-watch
            cargo-outdated

            # gdb-multitarget

            flip-link
            probe-run
            usbutils

            git
            # cmake
            minicom
            # expect

            dfu-util

            # bashInteractive
          ] ++ minBuildInputs;

          depsBuildBuild = with pkgs; [ qemu ];

          LD_LIBRARY_PATH = "${pkgs.stdenv.cc.cc.lib}/lib";

          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.stdenv.cc.targetPrefix}cc";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = "qemu-aarch64";
        };
      }
    );

}
