#!/usr/bin/env bash
set -euo pipefail

: "${HOMEBREW_TAP_GITHUB_TOKEN:?HOMEBREW_TAP_GITHUB_TOKEN is required}"

owner="vrypan"
repo="stash"
tap_repo="homebrew-tap"
tap_branch="main"
formula_name="stash"
package_name="stash-cli"
homepage="https://github.com/${owner}/${repo}"
description="A local store for pipeline output and ad hoc file snapshots."
license_name="MIT"
commit_name="Panayotis Vryonis"
commit_email="58812+vrypan@users.noreply.github.com"

tag="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "${tag}" ]]; then
  echo "usage: $0 <tag>" >&2
  exit 1
fi
version="${tag#v}"

asset_name() {
  local os="$1"
  local arch="$2"
  local target=""
  case "${os}/${arch}" in
    darwin/arm64) target="aarch64-apple-darwin" ;;
    darwin/x86_64) target="x86_64-apple-darwin" ;;
    linux/arm64) target="aarch64-unknown-linux-gnu" ;;
    linux/x86_64) target="x86_64-unknown-linux-gnu" ;;
    *)
      echo "unsupported os/arch: ${os}/${arch}" >&2
      exit 1
      ;;
  esac
  printf "%s-%s.tar.xz" "${package_name}" "${target}"
}

asset_url() {
  local os="$1"
  local arch="$2"
  printf "https://github.com/%s/%s/releases/download/%s/%s" \
    "${owner}" "${repo}" "${tag}" "$(asset_name "${os}" "${arch}")"
}

asset_sha256() {
  local os="$1"
  local arch="$2"
  curl -fsSL "$(asset_url "${os}" "${arch}")" | shasum -a 256 | awk '{print $1}'
}

darwin_amd64_sha="$(asset_sha256 darwin x86_64)"
darwin_arm64_sha="$(asset_sha256 darwin arm64)"
linux_amd64_sha="$(asset_sha256 linux x86_64)"
linux_arm64_sha="$(asset_sha256 linux arm64)"

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

git clone \
  "https://x-access-token:${HOMEBREW_TAP_GITHUB_TOKEN}@github.com/${owner}/${tap_repo}.git" \
  "${tmpdir}/tap"

formula_dir="${tmpdir}/tap/Formula"
mkdir -p "${formula_dir}"
formula_path="${formula_dir}/stash.rb"

cat >"${formula_path}" <<EOF
class Stash < Formula
  desc "${description}"
  homepage "${homepage}"
  version "${version}"
  license "${license_name}"

  on_macos do
    on_arm do
      url "$(asset_url darwin arm64)"
      sha256 "${darwin_arm64_sha}"
    end
    on_intel do
      url "$(asset_url darwin x86_64)"
      sha256 "${darwin_amd64_sha}"
    end
  end

  on_linux do
    on_arm do
      url "$(asset_url linux arm64)"
      sha256 "${linux_arm64_sha}"
    end
    on_intel do
      url "$(asset_url linux x86_64)"
      sha256 "${linux_amd64_sha}"
    end
  end

  def install
    if which("stash")
      installed = Utils.safe_popen_read("stash", "version").strip
      version_text =
        if installed =~ /\Astash (\S+)\z/
          Regexp.last_match(1)
        elsif installed =~ /\A\d+\.\d+\.\d+(?:[-+][^\s]+)?\z/
          installed
        end

      if version_text
        existing_version = Version.new(version_text)
        if existing_version < Version.new("0.5.0")
          odie <<~EOS
            stash #{existing_version} is installed.
            Automatic upgrade from versions older than 0.5.0 is not supported.
            Visit https://github.com/vrypan/stash for more info.
            To force the upgrade: brew uninstall stash first, then install again.
          EOS
        end
      end
    end

    bin.install "stash"
    pkgshare.install "scripts" if Dir.exist?("scripts")
    (bash_completion/"stash").write Utils.safe_popen_read(
      bin/"stash", "completion", "bash"
    )
    (zsh_completion/"_stash").write Utils.safe_popen_read(
      bin/"stash", "completion", "zsh"
    )
    (fish_completion/"stash.fish").write Utils.safe_popen_read(
      bin/"stash", "completion", "fish"
    )
  end

  test do
    system "#{bin}/stash", "version"
  end
end
EOF

git -C "${tmpdir}/tap" config user.name "${commit_name}"
git -C "${tmpdir}/tap" config user.email "${commit_email}"
git -C "${tmpdir}/tap" add "Formula/${formula_name}.rb"

if git -C "${tmpdir}/tap" diff --cached --quiet; then
  echo "Homebrew formula already up to date."
  exit 0
fi

git -C "${tmpdir}/tap" commit -m "Brew formula update for ${formula_name} version ${tag}"
git -C "${tmpdir}/tap" push origin "${tap_branch}"
