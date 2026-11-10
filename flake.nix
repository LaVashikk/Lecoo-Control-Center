{
  description = "Lecoo Control Center";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:

    let
      system = "x86_64-linux";
      overlays = [ rust-overlay.overlays.default ];
      pkgs = import nixpkgs { inherit system overlays; };

      rustToolchain = pkgs.rust-bin.stable.latest.default;

      rustPlatform = pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
      };
      lecoo-control-center = rustPlatform.buildRustPackage {
        pname = "lecoo-control-center";
        version = "0.5.2-beta";

        src = self;

        cargoLock.lockFile = ./Cargo.lock;

        # RUSTFLAGS = "-C target-cpu=native";

        # preferLocalBuild = true;
        # allowSubstitutes = false;

        cargoBuildFlags = [
          "-p"
          "lecoo-ec-daemon"
          "-p"
          "lecoo-ctrl"
        ];

        cargoTestFlags = [
          "-p"
          "lecoo-ec-daemon"
          "-p"
          "lecoo-ctrl"
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        installPhase = ''
          runHook preInstall

          install -Dm755 target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/lecoo-ec-daemon \
            $out/bin/lecoo-ec-daemon

          install -Dm755 target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/lecoo-ctrl \
            $out/bin/lecoo-ctrl

          runHook postInstall
        '';

        meta = {
          description = "Daemon and CLI for controlling Lecoo laptop hardware";
          homepage = "https://github.com/LaVashikk/Lecoo-Control-Center";
          license = pkgs.lib.licenses.mit;
          platforms = [ "x86_64-linux" ];
          mainProgram = "lecoo-ctrl";
        };
      };
    in
    {
      packages.${system} = {
        default = lecoo-control-center;
        lecoo-control-center = lecoo-control-center;
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.pkg-config
          pkgs.rust-analyzer
        ];
        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
      };

      nixosModules.default = import ./nixos-module.nix {
        package = lecoo-control-center;
      };
    };
}
