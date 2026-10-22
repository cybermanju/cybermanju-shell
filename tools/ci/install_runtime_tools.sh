#!/usr/bin/env bash
# Instala somente a partir dos arquivos oficiais Ubuntu, sem fontes de terceiros.
set -euo pipefail

profile=${1-}
[[ $# -eq 1 ]] || { echo "uso: $0 {framebuffer|module-staging|kbuild}" >&2; exit 2; }
case "$profile" in
  framebuffer) packages=(qemu-system-x86 grub-common grub-pc-bin xorriso mtools gcc binutils) ;;
  module-staging) packages=(qemu-system-x86 grub-common grub-pc-bin xorriso mtools linux-headers-6.8.0-147-generic=6.8.0-147.147 gcc-13 kmod gcc binutils) ;;
  kbuild) packages=(linux-headers-6.8.0-147-generic=6.8.0-147.147 gcc-13 kmod) ;;
  *) echo "perfil desconhecido: $profile" >&2; exit 2 ;;
esac
simulate=${CI_RUNTIME_TOOLS_SIMULATE:-0}
[[ "$simulate" == 0 || "$simulate" == 1 ]] || { echo "simulação deve ser 0 ou 1" >&2; exit 2; }

# A raiz alternativa permite fixtures de teste sem sudo, rede ou mudanças no host.
root=${CI_RUNTIME_TOOLS_ROOT:-/}
root=${root%/}
os_release="$root/etc/os-release"
keyring="$root/usr/share/keyrings/ubuntu-archive-keyring.gpg"
[[ -r "$os_release" ]] || { echo "os-release ausente" >&2; exit 1; }
# Lê apenas os campos necessários, sem source ou eval.
os_id= os_codename=
while IFS= read -r line || [[ -n "$line" ]]; do
  case "$line" in
    ID=*) os_id=${line#ID=} ;;
    VERSION_CODENAME=*) os_codename=${line#VERSION_CODENAME=} ;;
  esac
done < "$os_release"
case "$os_id" in \"*\") os_id=${os_id#\"}; os_id=${os_id%\"} ;; esac
case "$os_codename" in \"*\") os_codename=${os_codename#\"}; os_codename=${os_codename%\"} ;; esac
[[ "$os_id" == ubuntu && "$os_codename" == noble ]] || { echo "requer Ubuntu 24.04/noble" >&2; exit 1; }
[[ "$(dpkg --print-architecture)" == amd64 ]] || { echo "requer arquitetura amd64" >&2; exit 1; }
[[ -f "$keyring" && -r "$keyring" && -s "$keyring" ]] || { echo "keyring Ubuntu ausente ou vazio: $keyring" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# Os índices e pacotes são públicos; _apt precisa atravessar o diretório.
chmod 0755 "$tmp"
mkdir -p "$tmp/cache/archives/partial"
sources="$tmp/ubuntu.sources"
cat > "$sources" <<EOF_SOURCES
Types: deb
URIs: https://archive.ubuntu.com/ubuntu
Suites: noble noble-updates noble-backports
Components: main universe
Signed-By: $keyring

Types: deb
URIs: https://security.ubuntu.com/ubuntu
Suites: noble-security
Components: main universe
Signed-By: $keyring
EOF_SOURCES
base_apt_opts=(
  -o "Dir::Etc::sourcelist=$sources"
  -o Dir::Etc::sourceparts=-
  -o "Dir::Cache::archives=$tmp/cache/archives"
  -o Dir::Cache::pkgcache=
  -o Dir::Cache::srcpkgcache=
)
# Um mirror em sincronização pode entregar Release e Packages inconsistentes.
# Cada tentativa usa listas novas dentro do tmp, sem tocar índices do host nem
# reutilizar índices parciais/antigos. Assinaturas e Error-Mode=any permanecem.
for attempt in 1 2 3; do
  lists="$tmp/lists-$attempt"
  mkdir -p "$lists/partial"
  apt_opts=("${base_apt_opts[@]}" -o "Dir::State::lists=$lists")
  if apt-get "${apt_opts[@]}" -o APT::Update::Error-Mode=any update; then
    break
  else
    status=$?
  fi
  echo "APT update falhou na tentativa $attempt/3 (código $status)" >&2
  if [[ "$attempt" == 3 ]]; then
    exit "$status"
  fi
  sleep "$((attempt * 5))"
done
# Não repete install/dpkg: falhas de instalação continuam fatais.
if [[ "$simulate" == 1 ]]; then
  apt-get "${apt_opts[@]}" --simulate install --no-install-recommends "${packages[@]}"
else
  apt-get "${apt_opts[@]}" install -y --no-install-recommends "${packages[@]}"
fi
