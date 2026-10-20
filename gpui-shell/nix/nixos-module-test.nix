{ pkgs }:
let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;
  defaultPackage = pkgs.writeShellScriptBin "gpuishell" "exit 0";
  overridePackage = pkgs.writeShellScriptBin "gpuishell-override" "exit 0";
  fakeSelf.packages.${system}.default = defaultPackage;
  module = import ./nixos-module.nix { self = fakeSelf; };

  evaluate =
    moduleConfig:
    import (pkgs.path + "/nixos/lib/eval-config.nix") {
      inherit system;
      modules = [
        module
        ({ ... }: {
          nixpkgs.pkgs = pkgs;
          system.stateVersion = "24.11";
        })
        moduleConfig
      ];
    };

  disabled = evaluate { };
  enabled = evaluate { programs.gpuishell.enable = true; };
  sessionEnabled = evaluate {
    programs.gpuishell = {
      enable = true;
      autostart = true;
    };
  };
  autostartWithoutEnable = evaluate { programs.gpuishell.autostart = true; };
  overridden = evaluate {
    programs.gpuishell = {
      enable = true;
      package = overridePackage;
    };
  };

  sessionService = sessionEnabled.config.systemd.user.services.gpuishell;
  sessionUnit = sessionEnabled.config.systemd.user.units."gpuishell.service".text;
in
assert !(builtins.elem defaultPackage disabled.config.environment.systemPackages);
assert !(builtins.hasAttr "gpuishell" disabled.config.systemd.user.services);
assert builtins.elem defaultPackage enabled.config.environment.systemPackages;
assert !(builtins.hasAttr "gpuishell" enabled.config.systemd.user.services);
assert !(builtins.hasAttr "gpuishell" autostartWithoutEnable.config.systemd.user.services);
assert sessionService.wantedBy == [ "graphical-session.target" ];
assert sessionService.unitConfig.ConditionEnvironment == "WAYLAND_DISPLAY";
assert sessionService.unitConfig.PartOf == "graphical-session.target";
assert sessionService.serviceConfig.ExecStart == "${defaultPackage}/bin/gpuishell";
assert lib.hasInfix "ConditionEnvironment=WAYLAND_DISPLAY" sessionUnit;
assert lib.hasInfix "PartOf=graphical-session.target" sessionUnit;
assert lib.hasInfix "ExecStart=" sessionUnit;
assert builtins.elem overridePackage overridden.config.environment.systemPackages;
assert !(builtins.elem defaultPackage overridden.config.environment.systemPackages);
assert !(builtins.hasAttr "gpuishell" overridden.config.systemd.user.services);
pkgs.runCommand "gpuishell-nixos-module-evaluation" { } "touch $out"
