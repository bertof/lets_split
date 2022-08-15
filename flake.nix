{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks.url = "github:cachix/pre-commit-hooks.nix";
  };


  outputs = { self, nixpkgs, rust-overlay, flake-utils, pre-commit-hooks }:
    let
      # cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      extensions = [ "rust-src" ];
      targets = [ "x86_64-unknown-linux-gnu" "thumbv7em-none-eabihf" ];
      overlays = [
        rust-overlay.overlays.default
        (_: super: { rustc = super.rust-bin.stable.latest.default.override { inherit extensions targets; }; })
      ];
    in
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
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

        apps = {
          upload_usb = flake-utils.lib.mkApp {
            drv = pkgs.writeShellScriptBin "upload_usb" ''
              export PATH="${pkgs.lib.makeBinPath (minBuildInputs ++ [pkgs.dfu-util])}":$PATH
              cargo build --release --bin ''${1:-split}
              arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/split split.bin
              sudo dfu-util -a 0 -s 0x8000000 -RD split.bin
            ''
            ;
          };

          update_keyboard = flake-utils.lib.mkApp {
            drv = pkgs.writeShellScriptBin "upload_update_keyboard" ''
              export PATH="${pkgs.lib.makeBinPath (minBuildInputs ++ [pkgs.dfu-util])}":$PATH
              cargo build --release --bin ''${1:-split}
              arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/split split.bin

              echo Flashing pads until stop
              while true ; do
                sudo dfu-util -a 0 -s 0x8000000 -RD split.bin
                echo Retrying in 5 seconds
                sleep 5
              done
            ''
            ;
          };
        };

        checks = {
          pre-commit-check = pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              cargo-clippy = {
                enable = true;
                name = "clippy";
                description = "Lint Rust code.";
                entry = "${pkgs.rustc}/bin/cargo-clippy";
                files = "\\.rs$";
                pass_filenames = false;
              };
              cargo-rustfmt = {
                enable = true;
                name = "rustfmt";
                description = "Format Rust code.";
                entry = "${pkgs.rustc}/bin/cargo fmt -- --check --color always";
                files = "\\.rs$";
                pass_filenames = false;
              };
              nix-linter.enable = true;
              nixpkgs-fmt.enable = true;
              # clippy.enable = true;
              # rustfmt.enable = true;
            };
          };
        };

        devShells.default = pkgs.mkShell {
          shellHook = ''
            ${self.checks.${system}.pre-commit-check.shellHook}
          '';

          buildInputs = with pkgs; [
            rustc
            bacon
            cargo-watch
            cargo-outdated
            protobuf

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

          # LD_LIBRARY_PATH = "${pkgs.stdenv.cc.cc.lib}/lib";

          # CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "cc";
          # CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = "qemu-aarch64";
        };
      }
    );

}
