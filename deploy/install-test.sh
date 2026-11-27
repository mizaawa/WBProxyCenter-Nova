#!/usr/bin/env bash
# Install only the verified cloud build, never replace an existing site.
set -euo pipefail
umask 077
archive=wbpc-nova-linux-amd64.tar.gz
src=$(cd -- "${1:-.}" && pwd -P)
root=${WBP_WWWROOT:-/www/wwwroot}
addr=${WBP_PUBLISH_ADDR:-127.0.0.1}
port=${WBP_PUBLISH_PORT:-13040}
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'This build requires Linux x86-64.' >&2; exit 1; }
[[ -d $root && ! -L $root && -w $root ]] || { echo 'The selected web root must be a writable directory.' >&2; exit 1; }
[[ $addr =~ ^[0-9]{1,3}([.][0-9]{1,3}){3}$ && $addr != 0.0.0.0 ]] || { echo 'Specify a single local IPv4 address.' >&2; exit 1; }
if [[ ! $port =~ ^[0-9]{1,5}$ ]] || (( 10#$port < 1 || 10#$port > 65535 )); then echo 'Invalid TCP port.' >&2; exit 1; fi
for tool in docker sha256sum tar curl; do command -v "$tool" >/dev/null; done
docker compose version >/dev/null
docker info >/dev/null
# Restrict verification to the expected basename, not paths supplied in checksum files.
expected=$(awk -v name="$archive" '$2==name && length($1)==64 {print $1}' "$src/$archive.sha256")
[[ $expected =~ ^[[:xdigit:]]{64}$ ]] || { echo 'Invalid archive checksum file.' >&2; exit 1; }
(cd "$src" && printf '%s  %s\n' "$expected" "$archive" | sha256sum -c -)
project="$root/wbproxycenter-nova-test"
if [[ -e $project || -L $project ]]; then echo "Refusing to overwrite $project" >&2; exit 1; fi
if [[ -n $(docker ps -aq --filter label=com.docker.compose.project=wbpc-nova-test) ]]; then echo 'The test Compose project already exists.' >&2; exit 1; fi
mkdir -m 700 -- "$project"
cd -- "$project"
tar --no-same-owner --no-same-permissions -xzf "$src/$archive" ./wbpc-server ./wbpc-nova-image.tar.gz ./docker-compose.yaml ./README.md ./SHA256SUMS
sha256sum -c SHA256SUMS
printf 'WBP_PUBLISH_ADDR=%s\nWBP_PUBLISH_PORT=%s\n' "$addr" "$port" > .env
docker load -i wbpc-nova-image.tar.gz
docker compose up -d --no-build --pull never
url="http://$addr:$port"
curl --noproxy '*' --retry 25 --retry-connrefused --retry-delay 1 --max-time 5 -fsS "$url/health"
printf '\nTest console: %s/\nDirectory: %s\n' "$url" "$project"
