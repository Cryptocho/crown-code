{
  description = "crown-code";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" "llvm-tools-preview" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
          inherit src;
        };

        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          pname = "crown-workspace";
          version = "0.1.0";
          cargoExtraArgs = "--workspace";
        });

        core = craneLib.buildPackage (commonArgs // {
          pname = "crown-core";
          version = "0.1.0";
          cargoExtraArgs = "-p crown-core";
          inherit cargoArtifacts;
        });

        tui = craneLib.buildPackage (commonArgs // {
          pname = "crown-tui";
          version = "0.1.0";
          cargoExtraArgs = "-p crown-tui";
          inherit cargoArtifacts;
        });
      in
      {
        packages = {
          default = core;
          inherit core tui;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ core tui ];
          packages = with pkgs; [
            rustToolchain
            cargo-llvm-cov
          ];

          shellHook = ''
            echo "╔══════════════════════════════════════════╗"
            echo "║        crown-code dev environment        ║"
            echo "╚══════════════════════════════════════════╝"
          '';
        };
      }
    );
}