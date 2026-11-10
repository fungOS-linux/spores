#!/bin/sh
# Install files.myc as a user service and publish it to Mycelium.
#
#   tools/install.sh [--vfs [name=]dir]... [--root name=path]... [--bind ADDR:PORT]
#                    [--advertise URL]... [--no-anycast]
#                    [--binary PATH] [--sdk DIR] [--lua PATH]
#                    [--routes PATH] [--no-gateway] [--mycelium-bin PATH]
#                    [--uninstall] [--dry-run]
#
# The service is per-user on purpose: the data it counts is the user's, and a
# per-user LaunchAgent/systemd unit needs no password and no root-owned files.
# It reads the fungOS VFS by default, binds every interface, and writes a
# renewable application lease, so `files.myc` can resolve to whichever machine
# is running the service. Pass --bind 127.0.0.1:8803 or --no-anycast to keep it
# on this host; --root is the opt-in local-directory mode, not the default.
#
# What it installs under ${FUNGOS_HOME:-~/.local/share/fungos}/:
#
#   bin/files-serve           the host binary
#   files/run.sh              the exact command the service runs
#   files/ui/files.lua        the view Canvas and the browser both draw
#   files/theme.lua           the palette the scene is resolved against
#   files/shot.lua            the scene renderer (tools/shot.lua)
#   files/sdk/lua/fungos/ui.lua   the one library the view requires
#   files/files.log           stdout/stderr
#
# The copies make the service independent of this checkout and of the SDK
# checkout it was built from: after install, only `lua` and these files matter.
#
# Mycelium reads $MYCELIUM_HOME/application-services.json during a scan and
# gossips each fresh observation. The service renews its own entry every minute
# and runs `mycelium scan`, so the lease is visible to every gateway on the mesh
# without this installer having to write a second route. The local gateway still
# gets a plain loopback upstream: this machine is the first candidate, peers are
# the fallback.
set -eu

command -v curl >/dev/null 2>&1 || { printf 'files-install: curl is required for verification\n' >&2; exit 1; }

LABEL=dev.fungos.files
UNIT=files.service
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SPORE=$(CDPATH= cd -- "$HERE/.." && pwd)
REPO=$(CDPATH= cd -- "$SPORE/../.." && pwd)
FUNGOS_HOME=${FUNGOS_HOME:-$HOME/.local/share/fungos}
BIN_DIR=$FUNGOS_HOME/bin
STATE=$FUNGOS_HOME/files
LOG=$STATE/files.log
BIND=0.0.0.0:8803
BIND_SET=0
BINARY=
SDK=${FUNGOS_SDK:-}
LUA_BIN=
MYCELIUM_HOME=${MYCELIUM_HOME:-$HOME/.mycelium}
ROUTES=$MYCELIUM_HOME/gateway/routes.json
MYCELIUM_BIN=${MYCELIUM_BIN:-$MYCELIUM_HOME/bin/mycelium}
LEASE_FILE=$MYCELIUM_HOME/application-services.json
REMOTE_TOKEN=$MYCELIUM_HOME/gateway/files-remote-token
WITH_GATEWAY=1
WITH_ANYCAST=1
UNINSTALL=0
DRY_RUN=0
ROOTS=
NODE_ID=
ANYCAST_ENDPOINTS=

say() { printf 'files-install: %s\n' "$*"; }
warn() { printf 'files-install: warning: %s\n' "$*" >&2; }
die() { printf 'files-install: %s\n' "$*" >&2; exit 1; }

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
}

# One argument with nothing left for the shell to split. Paths may contain
# spaces and quotes; a generated run.sh must reproduce exactly what was chosen.
quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

xml_escape() { printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'; }

bind_host() {
  case $1 in
    \[*\]:*) printf '%s' "$1" | sed 's/^\[\(.*\)\]:.*/\1/' ;;
    *:*) printf '%s' "${1%:*}" ;;
    *) printf '%s' "$1" ;;
  esac
}

bind_port() {
  case $1 in
    *:*) printf '%s' "${1##*:}" ;;
    *) printf '' ;;
  esac
}

# Addresses a peer can dial. Keep the list to interfaces a person would expect
# another machine to find: no loopback, no link-local, no container bridges
# whose address means nothing outside this kernel.
detect_addresses() {
  case $(uname -s) in
    Darwin)
      ifconfig -a 2>/dev/null | awk '
        /^[A-Za-z0-9]+:/ { iface = $1; sub(":$", "", iface) }
        /inet / {
          if (iface ~ /^(lo|bridge|vmnet|awdl|llw|ap|gif|stf)/) next
          ip = $2
          if (ip ~ /^127\./ || ip ~ /^169\.254\./) next
          print ip
        }'
      ;;
    Linux)
      if command -v ip >/dev/null 2>&1; then
        ip -o -4 addr show 2>/dev/null | awk '
          $2 !~ /^(lo|docker|veth|br-|virbr|podman|cni|flannel|cali)/ {
            split($4, addr, "/")
            if (addr[1] !~ /^127\./) print addr[1]
          }'
      elif command -v hostname >/dev/null 2>&1; then
        hostname -I 2>/dev/null | tr ' ' '\n' | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$' | grep -v '^127\.' || true
      fi
      ;;
  esac
}

ROOTS_FILE=$(mktemp "${TMPDIR:-/tmp}/files-roots.XXXXXX")
VFS_FILE=$(mktemp "${TMPDIR:-/tmp}/files-vfs.XXXXXX")
ADVERTISE_FILE=$(mktemp "${TMPDIR:-/tmp}/files-advertise.XXXXXX")
trap 'rm -f "$ROOTS_FILE" "$VFS_FILE" "$ADVERTISE_FILE"' EXIT INT TERM

add_root() {
  name=$1
  path=$2
  [ -d "$path" ] || die "root $path is not a directory"
  [ -n "$name" ] || name=files
  printf '%s=%s\n' "$name" "$(CDPATH= cd -- "$path" && pwd)" >> "$ROOTS_FILE"
}

add_vfs() {
  name=$1
  path=$2
  [ -n "$name" ] || name=VFS
  [ -f "$path/.fungos/vfs.sqlite3" ] \
    || die "VFS $path has no .fungos/vfs.sqlite3"
  printf '%s=%s\n' "$name" "$(CDPATH= cd -- "$path" && pwd)" >> "$VFS_FILE"
}

while [ $# -gt 0 ]; do
  case $1 in
    --vfs)
      [ $# -ge 2 ] || die "--vfs needs a value"
      value=$2
      shift 2
      case $value in
        *=*) add_vfs "${value%%=*}" "${value#*=}" ;;
        *) add_vfs VFS "$value" ;;
      esac
      ;;
    --root)
      [ $# -ge 2 ] || die "--root needs a value"
      value=$2
      shift 2
      case $value in
        *=*) add_root "${value%%=*}" "${value#*=}" ;;
        *) add_root "$(basename "$value" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-')" "$value" ;;
      esac
      ;;
    --bind) [ $# -ge 2 ] || die "--bind needs a value"; BIND=$2; BIND_SET=1; shift 2 ;;
    --binary) [ $# -ge 2 ] || die "--binary needs a value"; BINARY=$2; shift 2 ;;
    --sdk) [ $# -ge 2 ] || die "--sdk needs a value"; SDK=$2; shift 2 ;;
    --lua) [ $# -ge 2 ] || die "--lua needs a value"; LUA_BIN=$2; shift 2 ;;
    --routes) [ $# -ge 2 ] || die "--routes needs a value"; ROUTES=$2; shift 2 ;;
    --advertise)
      [ $# -ge 2 ] || die "--advertise needs a URL"
      case $2 in
        http://*|https://*) printf '%s\n' "$2" >> "$ADVERTISE_FILE" ;;
        *) die "--advertise $2: expected an http(s) URL" ;;
      esac
      shift 2
      ;;
    --mycelium-bin) [ $# -ge 2 ] || die "--mycelium-bin needs a value"; MYCELIUM_BIN=$2; shift 2 ;;
    --no-anycast) WITH_ANYCAST=0; shift ;;
    --no-gateway) WITH_GATEWAY=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument: $1 (try --help)" ;;
  esac
done

os=$(uname -s)
case $os in
  Darwin) PLIST=$HOME/Library/LaunchAgents/$LABEL.plist ;;
  Linux) SYSTEMD=$HOME/.config/systemd/user/$UNIT ;;
  *) die "unsupported platform: $os (macOS and Linux user services are implemented)" ;;
esac

if [ "$UNINSTALL" -eq 1 ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    say "dry run: would remove the service, $STATE and $BIN_DIR/files-serve"
    [ "$WITH_GATEWAY" -eq 1 ] && say "dry run: would remove the files.myc route from $ROUTES"
    say "dry run: would remove the files application lease from $LEASE_FILE"
    say "dry run: would remove $REMOTE_TOKEN"
    exit 0
  fi
  if [ "$os" = Darwin ]; then
    launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    say "removed $PLIST"
  else
    systemctl --user disable --now "$UNIT" 2>/dev/null || true
    rm -f "$SYSTEMD"
    systemctl --user daemon-reload 2>/dev/null || true
    say "removed $SYSTEMD"
  fi
  if [ "$WITH_GATEWAY" -eq 1 ]; then
    remove_route=1
  else
    remove_route=0
  fi
  if [ "$remove_route" -eq 1 ] && [ -f "$ROUTES" ] && command -v python3 >/dev/null 2>&1; then
    cp "$ROUTES" "$ROUTES.bak-before-files-$(date +%s)"
    python3 - "$ROUTES" <<'PY'
import json, os, sys, tempfile
path = sys.argv[1]
with open(path) as handle:
    document = json.load(handle)
before = len(document.get("routes", []))
document["routes"] = [route for route in document.get("routes", []) if route.get("name") != "files"]
if len(document["routes"]) != before:
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".routes-")
    with os.fdopen(fd, "w") as handle:
        handle.write(json.dumps(document, indent=2) + "\n")
    os.chmod(tmp, 0o644)
    os.replace(tmp, path)
    print("files-install: removed the files.myc gateway route")
PY
  fi
  rm -rf "$STATE"
  rm -f "$BIN_DIR/files-serve"
  say "uninstalled (the data it counted is untouched)"
  exit 0
fi

# Local-only is the whole contract when anycast is off, so do not keep the
# broad bind unless the operator asked for a specific address.
if [ "$WITH_ANYCAST" -eq 0 ] && [ "$BIND_SET" -eq 0 ]; then
  BIND=127.0.0.1:8803
fi

PORT=$(bind_port "$BIND")
case $PORT in
  ''|*[!0-9]*) die "--bind $BIND: expected ADDR:PORT with a numeric port" ;;
esac
BIND_HOST=$(bind_host "$BIND")

if [ "$WITH_ANYCAST" -eq 1 ]; then
  if [ ! -x "$MYCELIUM_BIN" ]; then
    warn "no mycelium CLI at $MYCELIUM_BIN; the service will stay local (use --mycelium-bin or --no-anycast)"
    WITH_ANYCAST=0
  elif ! command -v python3 >/dev/null 2>&1; then
    warn "python3 is required to read the Mycelium node id; the service will stay local"
    WITH_ANYCAST=0
  else
    NODE_ID=$("$MYCELIUM_BIN" node status --json 2>/dev/null \
      | python3 -c 'import json,sys; print(json.load(sys.stdin).get("node",{}).get("hello",{}).get("node_id",""))' 2>/dev/null || true)
    case $NODE_ID in
      *[!0-9a-f]*|'')
        warn "could not read a Mycelium node id; the service will stay local"
        WITH_ANYCAST=0
        ;;
    esac
  fi
fi

if [ "$WITH_ANYCAST" -eq 1 ]; then
  if [ -s "$ADVERTISE_FILE" ]; then
    sort -u -o "$ADVERTISE_FILE" "$ADVERTISE_FILE"
  else
    case $BIND_HOST in
      0.0.0.0|::|'')
        detect_addresses | sort -u | while IFS= read -r address; do
          [ -n "$address" ] && printf 'http://%s:%s\n' "$address" "$PORT"
        done > "$ADVERTISE_FILE"
        ;;
      *)
        printf 'http://%s:%s\n' "$BIND_HOST" "$PORT" > "$ADVERTISE_FILE"
        ;;
    esac
  fi
  if [ ! -s "$ADVERTISE_FILE" ]; then
    printf 'http://127.0.0.1:%s\n' "$PORT" >> "$ADVERTISE_FILE"
    warn "no non-loopback address found; the lease will only reach the local gateway"
  fi
  ANYCAST_ENDPOINTS=$(tr '\n' ' ' < "$ADVERTISE_FILE" | sed 's/ $//')
fi

# The VFS is the point. A machine with a standard workspace should show it
# without the operator having to name a path; local folders remain an opt-in
# fallback for machines that do not have one yet.
if [ ! -s "$ROOTS_FILE" ] && [ ! -s "$VFS_FILE" ]; then
  if [ -f "$FUNGOS_HOME/vfs/.fungos/vfs.sqlite3" ]; then
    add_vfs VFS "$FUNGOS_HOME/vfs"
  else
    warn "no VFS at $FUNGOS_HOME/vfs; falling back to local folders (pass --vfs DIR to choose one)"
    for candidate in "$HOME/src" "$HOME/Documents" "$HOME/Pictures" "$HOME/Downloads"; do
      if [ -d "$candidate" ]; then
        name=$(basename "$candidate" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-')
        add_root "$name" "$candidate"
      fi
    done
    if [ ! -s "$ROOTS_FILE" ]; then
      add_root home "$HOME"
    fi
  fi
fi

if [ -z "$SDK" ]; then
  for candidate in "$HOME/src/fungos-sdk" "$HOME/src/fungOS/fungos-sdk" "$HOME/fungos-sdk"; do
    if [ -f "$candidate/lua/fungos/ui.lua" ]; then SDK=$candidate; break; fi
  done
fi
[ -n "$SDK" ] && [ -f "$SDK/lua/fungos/ui.lua" ] \
  || die "no fungos.ui; set FUNGOS_SDK to a fungos-sdk checkout (or pass --sdk DIR)"

if [ -z "$LUA_BIN" ]; then
  for candidate in lua lua5.4 lua5.3 luajit; do
    if command -v "$candidate" >/dev/null 2>&1; then LUA_BIN=$(command -v "$candidate"); break; fi
  done
fi
[ -n "$LUA_BIN" ] || die "no Lua interpreter on PATH; pass --lua /path/to/lua"

if [ -z "$BINARY" ]; then
  [ "$DRY_RUN" -eq 1 ] || say "building $REPO/spores/files with cargo"
  if [ "$DRY_RUN" -eq 1 ]; then
    BINARY=$REPO/target/release/files-serve
  else
    (cd "$REPO" && cargo build -q -p spore-files --release) || die "cargo build failed"
    BINARY=$REPO/target/release/files-serve
  fi
fi
[ -x "$BINARY" ] || [ "$DRY_RUN" -eq 1 ] || die "$BINARY is not executable"

# The route's upstream is loopback even when the listener was told 0.0.0.0: a
# wildcard bind is about which peers may reach it, not a host to dial.
case $BIND in
  0.0.0.0:*|\[::\]:*) UPSTREAM=http://127.0.0.1:${BIND##*:} ;;
  \[*\]:*) UPSTREAM=http://${BIND} ;;
  *) UPSTREAM=http://$BIND ;;
esac

run_command() {
  printf '#!/bin/sh\n'
  printf '# Generated by spores/files/tools/install.sh -- edit locations by re-running it.\n'
  printf 'exec '
  quote "$BIN_DIR/files-serve"
  while IFS= read -r root; do
    printf ' %s %s' "$(quote --root)" "$(quote "$root")"
  done < "$ROOTS_FILE"
  while IFS= read -r vfs; do
    printf ' %s %s' "$(quote --vfs)" "$(quote "$vfs")"
  done < "$VFS_FILE"
  printf ' %s %s' "$(quote --bind)" "$(quote "$BIND")"
  printf ' %s %s' "$(quote --sdk)" "$(quote "$STATE/sdk")"
  printf ' %s %s' "$(quote --spore-root)" "$(quote "$STATE")"
  printf ' %s %s' "$(quote --theme)" "$(quote "$STATE/theme.lua")"
  printf ' %s %s' "$(quote --renderer)" "$(quote "$STATE/shot.lua")"
  printf ' %s %s' "$(quote --lua)" "$(quote "$LUA_BIN")"
  printf ' %s %s' "$(quote --refresh)" "120"
  if [ "$WITH_ANYCAST" -eq 1 ]; then
    printf ' %s %s' "$(quote --lease-file)" "$(quote "$LEASE_FILE")"
    printf ' %s %s' "$(quote --lease-kind)" "files"
    printf ' %s %s' "$(quote --lease-node)" "$(quote "$NODE_ID")"
    printf ' %s %s' "$(quote --lease-label)" "$(quote "$(hostname -s 2>/dev/null || hostname)")"
    while IFS= read -r endpoint; do
      [ -n "$endpoint" ] && printf ' %s %s' "$(quote --lease-endpoint)" "$(quote "$endpoint")"
    done < "$ADVERTISE_FILE"
    printf ' %s' "$(quote --lease-scan)"
    printf ' %s %s' "$(quote --lease-refresh)" "60"
    printf ' %s %s' "$(quote --mycelium-bin)" "$(quote "$MYCELIUM_BIN")"
  fi
  printf '\n'
}

write_run() {
  install -d "$STATE"
  run_command > "$STATE/run.sh"
  chmod 0755 "$STATE/run.sh"
}

write_plist() {
  cat > "$PLIST" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$LABEL</string>
<key>ProgramArguments</key><array><string>/bin/sh</string><string>$(xml_escape "$STATE/run.sh")</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><true/>
<key>ThrottleInterval</key><integer>5</integer>
<key>StandardOutPath</key><string>$(xml_escape "$LOG")</string>
<key>StandardErrorPath</key><string>$(xml_escape "$LOG")</string>
</dict></plist>
XML
}

write_unit() {
  cat > "$SYSTEMD" <<UNITFILE
[Unit]
Description=files.myc — VFS browser and storage census
After=network.target

[Service]
ExecStart=/bin/sh "$STATE/run.sh"
Restart=always
RestartSec=5
StandardOutput=append:$LOG
StandardError=append:$LOG

[Install]
WantedBy=default.target
UNITFILE
}

update_route() {
  [ "$WITH_GATEWAY" -eq 1 ] || { say "gateway route skipped (--no-gateway)"; return 0; }
  if [ ! -f "$ROUTES" ]; then
    warn "no gateway routes at $ROUTES; add the files.myc route by hand"
    return 0
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    warn "python3 is not on PATH; add this route by hand: files.myc -> $UPSTREAM"
    return 0
  fi
  cp "$ROUTES" "$ROUTES.bak-before-files-$(date +%s)"
  token=
  [ "$WITH_ANYCAST" -eq 1 ] && token=$REMOTE_TOKEN
  python3 - "$ROUTES" "$UPSTREAM" "$token" <<'PY'
import json, os, sys, tempfile
path, upstream, token = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path) as handle:
    document = json.load(handle)
route = {
    "name": "files",
    "hosts": ["files.myc"],
    "upstream": upstream,
    "service_kind": "files",
    "priority": 10,
    "dock": True,
}
if token:
    # The gateway only dials a discovered peer when the route names a remote
    # credential; it must never replay a caller's credential to a stranger.
    # files-serve is read-only and does not require the token, but the route
    # needs one before the mesh half of the anycast is eligible.
    route["remote_token_file"] = token
routes = document.setdefault("routes", [])
for existing in routes:
    if existing.get("name") == "files":
        existing.pop("remote_token_file", None)
        existing.update(route)
        break
else:
    routes.append(route)
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".routes-")
with os.fdopen(fd, "w") as handle:
    handle.write(json.dumps(document, indent=2) + "\n")
os.chmod(tmp, 0o644)
os.replace(tmp, path)
print("files-install: route files.myc ->", upstream)
PY
}

verify() {
  port=$PORT
  tries=0
  while [ "$tries" -lt 40 ]; do
    if curl -fsS --max-time 2 "http://127.0.0.1:$port/" >/dev/null 2>&1; then break; fi
    tries=$((tries + 1))
    sleep 0.25
  done
  curl -fsS --max-time 3 "http://127.0.0.1:$port/" >/dev/null \
    || die "the service did not answer on http://127.0.0.1:$port/ (see $LOG)"
  say "local service: http://127.0.0.1:$port/"

  if [ "$WITH_ANYCAST" -eq 1 ]; then
    # The service publishes first and runs `mycelium scan` beside it. A cold
    # scan can take a few seconds, so wait for the catalog rather than reading
    # the absence at t+1 as a failure.
    tries=0
    while [ "$tries" -lt 90 ]; do
      if "$MYCELIUM_BIN" services --kind files --json 2>/dev/null | grep -q "$NODE_ID"; then
        say "anycast: kind=files node=$NODE_ID endpoints=$ANYCAST_ENDPOINTS"
        break
      fi
      tries=$((tries + 1))
      sleep 1
    done
    if [ "$tries" -ge 90 ]; then
      warn "the files lease is not in 'mycelium services --kind files' yet; check $LEASE_FILE and run '$MYCELIUM_BIN scan'"
    fi
  fi

  if [ "$WITH_GATEWAY" -eq 1 ]; then
    # A cold scene scans every location, and a large tree takes seconds. The
    # browser waits; the installer waits with it rather than calling a working
    # route broken.
    tries=0
    while [ "$tries" -lt 80 ]; do
      if curl -kfsS --max-time 15 "https://files.myc/scene.json?w=800&h=600" >/dev/null 2>&1; then
        say "gateway: https://files.myc/"
        return 0
      fi
      tries=$((tries + 1))
      sleep 0.25
    done
    warn "https://files.myc/ did not answer; check 'mycelium gateway status' and $ROUTES"
  fi
}

if [ "$DRY_RUN" -eq 1 ]; then
  say "dry run"
  say "  binary:   $BINARY -> $BIN_DIR/files-serve"
  say "  vfs:      $(tr '\n' ' ' < "$VFS_FILE")"
  say "  roots:    $(tr '\n' ' ' < "$ROOTS_FILE")"
  say "  bind:     $BIND"
  say "  sdk:      $SDK (one file is copied)"
  say "  lua:      $LUA_BIN"
  say "  service:  $([ "$os" = Darwin ] && echo "$PLIST" || echo "$SYSTEMD")"
  say "  route:    $ROUTES  files.myc -> $UPSTREAM"
  if [ "$WITH_ANYCAST" -eq 1 ]; then
    say "  anycast:  kind=files node=$NODE_ID"
    say "  lease:    $LEASE_FILE"
    say "  endpoints: $ANYCAST_ENDPOINTS"
    say "  token:    $REMOTE_TOKEN"
  else
    say "  anycast:  off"
  fi
  say "  run.sh:"
  run_command | sed 's/^/    /'
  exit 0
fi

install -d "$BIN_DIR" "$STATE/ui" "$STATE/sdk/lua/fungos"
install -m 0755 "$BINARY" "$BIN_DIR/files-serve"
install -m 0644 "$SPORE/ui/files.lua" "$STATE/ui/files.lua"
install -m 0644 "$SPORE/../themes/graphite.lua" "$STATE/theme.lua"
install -m 0644 "$REPO/tools/shot.lua" "$STATE/shot.lua"
install -m 0644 "$SDK/lua/fungos/ui.lua" "$STATE/sdk/lua/fungos/ui.lua"
write_run
if [ "$WITH_ANYCAST" -eq 1 ] && [ "$WITH_GATEWAY" -eq 1 ]; then
  install -d "$MYCELIUM_HOME/gateway"
  if [ ! -s "$REMOTE_TOKEN" ]; then
    (umask 077; od -An -N32 -tx1 /dev/urandom | tr -d ' \n' > "$REMOTE_TOKEN")
  fi
  chmod 600 "$REMOTE_TOKEN"
fi
say "installed $BIN_DIR/files-serve and $STATE"

if [ "$os" = Darwin ]; then
  install -d "$(dirname "$PLIST")"
  write_plist
  launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$PLIST"
  launchctl kickstart -k "gui/$(id -u)/$LABEL" 2>/dev/null || true
  say "service $LABEL loaded"
else
  install -d "$(dirname "$SYSTEMD")"
  write_unit
  systemctl --user daemon-reload
  systemctl --user enable "$UNIT" >/dev/null
  systemctl --user restart "$UNIT"
  say "service $UNIT enabled"
fi

update_route
verify

if [ "$os" = Darwin ] && [ -s "$ROOTS_FILE" ]; then
  say "macOS withholds protected folders (Photos Library, Music, Downloads, Documents) from a background service reading --root folders. The footer reports what it could not read; grant Full Disk Access to $BIN_DIR/files-serve and restart the service to count them."
fi
