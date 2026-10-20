{
  pkgs ? import ../nixpkgs.nix { },
  autoInstall ? false,
  extra-substituters ? "",
  config-file-name ? "configuration.nix",
  extra-trusted-public-keys ? "",
  target_platform ? "x86_64-linux",
  version ? "",
  ...
}:
let
  installer = pkgs.callPackage ../aster_nixos_installer {
    inherit
      extra-substituters
      extra-trusted-public-keys
      config-file-name
      target_platform
      ;
  };
  configuration = {
    imports = [
      "${pkgs.path}/nixos/modules/installer/cd-dvd/installation-cd-minimal.nix"
      "${pkgs.path}/nixos/modules/installer/cd-dvd/channel.nix"
    ];

    system.nixos.distroName = "Asterinas NixOS";
    system.nixos.label = "${version}";
    isoImage.appendToMenuLabel = " Installer";
    # Keep the full minimal installer, nixpkgs source and editable target config.
    # The live environment uses Linux; Asterinas is the installed target kernel.
    isoImage.makeBiosBootable = true;
    isoImage.makeEfiBootable = true;
    isoImage.makeUsbBootable = true;
    isoImage.squashfsCompression = "xz -Xdict-size 100%";
    isoImage.includeSystemBuildDependencies = false;
    isoImage.storeContents = [ (builtins.storePath pkgs.path) ];

    # No copytoram: the compressed store stays on the ISO/USB medium.
    boot.kernelParams = [ "console=tty0" "console=hvc0" ];
    services.getty.autologinUser = pkgs.lib.mkForce "root";
    environment.systemPackages = [ installer ];
    environment.loginShellInit = ''
      if [ ! -e "$HOME/configuration.nix" ]; then
        # Create an editable copy of configuration.nix in user's home.
        cp -L ${installer}/etc_nixos/configuration.nix $HOME && chmod u+w $HOME/configuration.nix
      fi

      if [ "$(tty)" == "/dev/hvc0" ]; then
        echo "CYBERMANJU_LIVE_INSTALLER_READY"
      fi

      ${pkgs.lib.optionalString autoInstall ''
        if [ "$(tty)" == "/dev/hvc0" ]; then
          echo "The installer automatically runs on /dev/hvc0!"
          aster-nixos-install --config $HOME/configuration.nix --disk /dev/vda --force-format-disk || true
          poweroff
        fi
      ''}
    '';
  };
in
(pkgs.nixos configuration).config.system.build.isoImage
