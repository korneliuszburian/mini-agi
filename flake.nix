{
  description = "mini-agi — single-binary agent kernel (memory, evals, skills, orchestration)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        # Pinned toolchain matching rust-toolchain.toml (1.97.1).
        rust = pkgs.rust-bin.stable."1.97.1".default.override {
          extensions = [ "rustfmt" "clippy" ];
        };
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "mini-agi";
          version = "0.3.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ rust ];
          # The kernel uses sha2 with explicit feature selection; default
          # cargo build already passes --locked in CI.
          doCheck = true;
          checkPhase = "cargo test --all";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [ rust ];
          shellHook = ''
            echo "mini-agi dev shell (rust 1.97.1 pinned) — run scripts/verify.sh"
          '';
        };
      });
}
