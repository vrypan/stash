#!/usr/bin/env bash
# Release stash locally.
# Usage: ./scripts/release.sh
#
# Requires: zig, gh (GitHub CLI, authenticated), HOMEBREW_TAP_GITHUB_TOKEN env var
set -euo pipefail

PACKAGE_NAME="stash"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ── Preflight ────────────────────────────────────────────────────────────────

for cmd in zig gh shasum; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: '$cmd' is required" >&2
    exit 1
  fi
done

if [[ -z "${HOMEBREW_TAP_GITHUB_TOKEN:-}" ]]; then
  echo "error: HOMEBREW_TAP_GITHUB_TOKEN is not set" >&2
  exit 1
fi

# ── Version ──────────────────────────────────────────────────────────────────

version="$(sed -n 's/^[[:space:]]*\.version = "\(.*\)",[[:space:]]*$/\1/p' build.zig.zon | head -n 1)"
if [[ -z "$version" ]]; then
  echo "error: could not read version from build.zig.zon" >&2
  exit 1
fi

tag="v${version}"
echo "Releasing ${tag}"

if git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
  echo "error: tag ${tag} already exists on origin" >&2
  exit 1
fi

# ── Build ────────────────────────────────────────────────────────────────────

targets=(
  "aarch64-linux-gnu"
  "x86_64-linux-gnu"
  "aarch64-macos"
  "x86_64-macos"
)

rm -rf dist
mkdir -p dist

for target in "${targets[@]}"; do
  echo "Building ${target}..."
  install_dir="$PWD/dist/install-${target}"
  package_dir="$PWD/dist/package-${target}"
  archive="$PWD/dist/${PACKAGE_NAME}-${target}.tar.xz"

  zig build \
    -Doptimize=ReleaseSmall \
    -Dtarget="$target" \
    -p "$install_dir"

  mkdir -p "$package_dir"
  cp "$install_dir/bin/stash"            "$package_dir/stash"
  cp "$install_dir/bin/stash-completion" "$package_dir/stash-completion"
  cp "$install_dir/bin/stash-bookmark"   "$package_dir/stash-bookmark"
  cp LICENSE README.md CHANGELOG.md      "$package_dir/"
  cp -R scripts                          "$package_dir/scripts"

  tar -C "$package_dir" -cJf "$archive" .
done

shasum -a 256 dist/*.tar.xz > dist/SHA256SUMS
echo "Archives and SHA256SUMS written to dist/"

# ── Tag and push ─────────────────────────────────────────────────────────────

git tag -a "$tag" -m "Release $tag"
git push origin "$tag"
echo "Tag ${tag} pushed"

# ── GitHub release ───────────────────────────────────────────────────────────

gh release create "$tag" \
  --title "$tag" \
  --notes "Release ${tag}" \
  dist/*.tar.xz \
  dist/SHA256SUMS

echo "GitHub release ${tag} created"

# ── Homebrew formula ─────────────────────────────────────────────────────────

echo "Updating Homebrew formula..."
HOMEBREW_TAP_GITHUB_TOKEN="$HOMEBREW_TAP_GITHUB_TOKEN" \
  .github/scripts/update-homebrew-formula.sh "$tag"

echo "Done."
