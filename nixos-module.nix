{ package }:

{ config, lib, ... }:

let
  cfg = config.services.lecoo-control-center;
in
{
  options.services.lecoo-control-center = {
    enable = lib.mkEnableOption "Lecoo Control Center";

    package = lib.mkOption {
      type = lib.types.package;
      default = package;
      defaultText = lib.literalExpression "the flake's lecoo-control-center package";
      description = "Lecoo Control Center package.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [
      cfg.package
    ];

    systemd.services.lecoo-daemon = {
      description = "Lecoo EC Control Daemon";

      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/lecoo-ec-daemon";

        Restart = "on-failure";
        RestartSec = "5s";

        User = "root";

        StateDirectory = "lecoo-control";
        StateDirectoryMode = "0755";
      };
    };
  };
}
