{
  description = "A very basic flake";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:nixos/nixpkgs";
    pre-commit-hooks-nix.url = "github:cachix/pre-commit-hooks.nix";
    rust-overlay.url = "github:oxalica/rust-overlay";
    systems.url = "github:nix-systems/default";
  };

  outputs = inputs: inputs.flake-parts.lib.mkFlake { inherit inputs; } {
    systems = import inputs.systems;
    imports = [
      # To import a flake module
      # 1. Add foo to inputs
      # 2. Add foo as a parameter to the outputs function
      # 3. Add here: foo.flakeModule
      inputs.pre-commit-hooks-nix.flakeModule
    ];
    perSystem =
      { config
        # , self'
        # , inputs'
      , pkgs
      , system
      , lib
      , ...
      }:
      let
        minBuildInputs = with pkgs; [
          gcc-arm-embedded
          flip-link
          stdenv.cc.cc.lib
          stdenv.cc
          git
          rustc
        ];
        uploadInputs = with pkgs; [
          dfu-util
        ];
      in
      {
        # Per-system attributes can be defined here. The self' and inputs'
        # module parameters provide easy access to attributes of the same
        # system.

        # This sets `pkgs` to a nixpkgs with allowUnfree option set.
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
            (self: _super: {
              rustc = self.rust-bin.stable.latest.default.override {
                extensions = [ "rust-src" ];
                targets = [ "x86_64-unknown-linux-gnu" "thumbv7em-none-eabihf" ];
              };
            })
          ];
          # config.allowUnfree = true;
        };

        apps = {
          upload_usb = {
            type = "app";
            program = pkgs.writeShellScript "upload_usb" ''
              set -e
              export PATH="${pkgs.lib.makeBinPath (minBuildInputs ++ uploadInputs)}":$PATH
              cargo build --release --bin ''${1:-split}
              arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/split split.bin
              sudo dfu-util -a 0 -s 0x8000000 -RD split.bin
            ''
            ;
          };
          update_keyboard = {
            type = "app";
            program = pkgs.writeShellScript "upload_update_keyboard" ''
              set -e
              export PATH="${pkgs.lib.makeBinPath (minBuildInputs ++ uploadInputs)}":$PATH
              cargo build --release --bin ''${1:-split}
              arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/split split.bin

              echo Flashing pads until stop
              while true ; do
                sudo dfu-util -a 0 -s 0x8000000 -RD split.bin || true
                echo Retrying in 5 seconds
                sleep 5
              done
            ''
            ;
          };
        };

        pre-commit = {
          settings = {
            hooks = {
              deadnix.enable = true;
              nixpkgs-fmt.enable = true;
              statix.enable = true;

              clippy.enable = true;
              rustfmt.enable = true;
              # cargo-test = {
              #   enable = true;
              #   name = "cargo test";
              #   description = "Test Rust code.";
              #   entry = toString (pkgs.writeShellScript "cargo test" ''
              #     export PATH=${lib.makeBinPath minBuildInputs}
              #     cargo test'');
              #   files = "\\.rs$";
              #   pass_filenames = false;
              # };
            };
            tools = {
              cargo = lib.mkForce pkgs.rustc;
              clippy = lib.mkForce pkgs.rustc;
              rustfmt = lib.mkForce pkgs.rustc;
            };
          };
        };

        devShells.default = pkgs.mkShell {
          shellHook = ''
            ${config.pre-commit.installationScript}
          '';

          buildInputs = minBuildInputs ++ uploadInputs ++ (
            with pkgs; [
              # cmake
              # expect
              # gdb-multitarget
              minicom
              probe-run
              usbutils
            ]
          );

          # depsBuildBuild = with pkgs; [ qemu ];

          # LD_LIBRARY_PATH = "${pkgs.stdenv.cc.cc.lib}/lib";

          # CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "cc";
          # CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = "qemu-aarch64";
        };

        formatter = pkgs.nixpkgs-fmt;
      };
    flake = {
      # The usual flake attributes can be defined here, including system-
      # agnostic ones like nixosModule and system-enumerating ones, although
      # those are more easily expressed in perSystem.
    };
  };
}
