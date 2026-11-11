# shellcheck shell=bash
# Sourced by the vm/* scripts.
VM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="$(cd "$VM_DIR/.." && pwd)"
STATE="$VM_DIR/.state"
TOOLS="$STATE/tools/bin"
SSH_KEY="$STATE/ssh/id_ed25519"
QMP_SOCK="$STATE/qmp.sock"
PID_FILE="$STATE/qemu.pid"
PORT_FILE="$STATE/ssh-port"
SCREEN_W=1920
SCREEN_H=1080

die() {
  echo "$(basename "$0"): $*" >&2
  exit 1
}

vm_running() {
  [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

require_running() {
  vm_running || die "the VM is not running; start it with vm/up"
}

vm_ssh() {
  "$TOOLS/ssh" -i "$SSH_KEY" -p "$(cat "$PORT_FILE")" \
    -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR -o BatchMode=yes -o ConnectTimeout=5 \
    -o ServerAliveInterval=10 -o ServerAliveCountMax=3 \
    kabilan@127.0.0.1 "$@"
}

qmp() {
  "$TOOLS/python3" -I "$VM_DIR/lib/qmp.py" --socket "$QMP_SOCK" \
    --width "$SCREEN_W" --height "$SCREEN_H" "$@"
}

# Quote argv for the guest's login shell.
quote_args() {
  local out="" arg
  for arg in "$@"; do
    out+="$(printf '%q' "$arg") "
  done
  printf '%s' "$out"
}

# The project is mounted at the same path in the guest, so a host cwd inside
# it is valid there too.
guest_cwd() {
  case "$PWD/" in
    "$PROJECT_DIR"/*) printf '%s' "$PWD" ;;
    *) printf '%s' "/home/kabilan" ;;
  esac
}
