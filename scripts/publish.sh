#!/usr/bin/env bash
set -euo pipefail
set +x

usage() {
  echo "Usage: $0 [--build-only] vMAJOR.MINOR.PATCH" >&2
  exit 2
}

build_only=false
if [[ ${1:-} == --build-only ]]; then
  build_only=true
  shift
fi
[[ $# == 1 && $1 =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || usage
version=${1#v}
tag=$1
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
cd "$root"

for tool in git docker tar; do
  command -v "$tool" >/dev/null || { echo "Missing tool: $tool" >&2; exit 1; }
done
if ! "$build_only"; then
  command -v op >/dev/null || { echo 'Missing tool: op' >&2; exit 1; }
fi

[[ -z $(git status --porcelain --untracked-files=all) ]] || {
  echo 'Commit or remove working-tree changes before publishing.' >&2
  exit 1
}
revision=$(git rev-parse --verify "refs/tags/$tag^{commit}")
[[ $(git rev-parse HEAD) == "$revision" ]] || {
  echo "Check out $tag before publishing; HEAD must match the release tag." >&2
  exit 1
}

umask 077
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/source" "$work/docker"
git archive "$revision" | tar -x -C "$work/source"

registry=registry.gsmn.dev
image="$registry/ddnser/ddnser:$version"
docker buildx build --platform linux/amd64 --load \
  --label "org.opencontainers.image.version=$version" \
  --label "org.opencontainers.image.revision=$revision" \
  --label 'org.opencontainers.image.source=https://github.com/thomasgassmann/ddnser' \
  --tag "$image" --file "$work/source/Dockerfile" "$work/source"

actual=$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image")
[[ $actual == "$revision" ]] || { echo "Revision mismatch: $image" >&2; exit 1; }
echo "Built $image from $revision"
"$build_only" && exit 0

# Docker credentials exist only in this private temporary directory.
op read --no-newline 'op://homelab/harbor-ddnser-push/password' \
  | docker --config "$work/docker" login "$registry" \
      --username 'robot$ddnser-push' --password-stdin
docker --config "$work/docker" push "$image"
echo 'Image pushed. Use the registry digest reported above in the deployment.'
