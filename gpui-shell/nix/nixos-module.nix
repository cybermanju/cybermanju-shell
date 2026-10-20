{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.gpuishell;
in
{
  options.programs.gpuishell = {
    enable = lib.mkEnableOption "GPUi Shell";

    autostart = lib.mkEnableOption "starting GPUi Shell in the Wayland graphical session";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "gpuishell.packages.\${pkgs.system}.default";
      description = "The GPUi Shell package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    systemd.user.services.gpuishell = lib.mkIf cfg.autostart {
      description = "GPUi Shell Wayland session shell";
      wantedBy = [ "graphical-session.target" ];
      unitConfig = {
        ConditionEnvironment = "WAYLAND_DISPLAY";
        PartOf = "graphical-session.target";
      };
      serviceConfig.ExecStart = lib.getExe cfg.package;
    };
  };
}
