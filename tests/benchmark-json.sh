#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

runtime=$(luajit -e 'io.write(jit.os, "/", jit.arch)')
case "$runtime" in
    OSX/arm*|OSX/aarch64) expected="mac-arm" ;;
    OSX/*) expected="mac-intel" ;;
    */arm*|*/aarch64) expected="linux-arm" ;;
    *) expected="linux-x86" ;;
esac

fixture=$(cat <<'JSON'
{
  "bottle": {
    "stable": {
      "files": {
        "arm64_sonoma": {"url": ""},
        "arm64_ventura": {"url": "mac-arm"},
        "x86_64_darwin": {"url": ""},
        "x86_64_ventura": {"url": "mac-intel"},
        "arm64_linux": {"url": ""},
        "arm64_linux_musl": {"url": "linux-arm"},
        "x86_64_linux": {"url": ""},
        "x86_64_linux_musl": {"url": "linux-x86"},
        "all": {"url": "universal"}
      }
    }
  }
}
JSON
)
actual=$(printf '%s' "$fixture" | luajit scripts/benchmark_json.lua bottle-url)
test "$actual" = "$expected"

fallback=$(cat <<'JSON'
{"bottle":{"stable":{"files":{"other_linux":{"url":"first-key"},"all":{"url":"universal"}}}}}
JSON
)
actual=$(printf '%s' "$fallback" | luajit scripts/benchmark_json.lua bottle-url)
test "$actual" = "universal"

token_json='{"token":"fixture-token"}'
actual=$(printf '%s' "$token_json" | luajit scripts/benchmark_json.lua token)
test "$actual" = "fixture-token"

token_json='{"token":null}'
actual=$(printf '%s' "$token_json" | luajit scripts/benchmark_json.lua token)
test -z "$actual"

if printf '{"token":' | luajit scripts/benchmark_json.lua token >/dev/null 2>&1; then
    echo "malformed token JSON unexpectedly succeeded" >&2
    exit 1
fi

echo "benchmark JSON helper tests passed ($runtime)"
