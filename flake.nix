{
  description = "Lecoo Control Center";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };

      lecoo-control-center = pkgs.rustPlatform.buildRustPackage {
        pname = "lecoo-control-center";
        version = "0.5.2-beta";

        src = self;

        cargoLock.lockFile = ./Cargo.lock;

        RUSTFLAGS = "-C target-cpu=native";

        preferLocalBuild = true;
        allowSubstitutes = false;

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
      nixosModules.default = import ./nixos-module.nix {
        package = lecoo-control-center;
      };
    };
}
