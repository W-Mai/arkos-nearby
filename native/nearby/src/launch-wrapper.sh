if [ "$?" -eq "10" ]; then
  # ARKOS_NEARBY_X_BEGIN
  sudo -n /opt/arkos-nearby/arkos-nearby menu --frontend "$emulator" -- "$@"
  result=$?
  if [ "$result" -ne "230" ]; then
    exit "$result"
  fi
  # ARKOS_NEARBY_X_END
fi

nearby_arguments=$(mktemp /tmp/nearby-arguments.XXXXXX) || exit 1
if [ -n "${ARKOS_NEARBY_SAVE_TICKET:-}" ]; then
  set -- "--nearby-save-ticket=$ARKOS_NEARBY_SAVE_TICKET" "$@"
fi
if sudo -n /opt/arkos-nearby/arkos-nearby resolve-launch --frontend "$emulator" -- "$@" > "$nearby_arguments"; then
  nearby_launch=()
  while IFS= read -r -d '' nearby_argument; do
    nearby_launch+=("$nearby_argument")
  done < "$nearby_arguments"
  rm -f -- "$nearby_arguments"
else
  nearby_status=$?
  rm -f -- "$nearby_arguments"
  exit "$nearby_status"
fi
if [ "${#nearby_launch[@]}" -lt "4" ]; then
  exit 1
fi
nearby_launch_id=${nearby_launch[0]}
emulator=${nearby_launch[1]}
nearby_runtime=${nearby_launch[2]}
set -- "${nearby_launch[@]:3}"
trap 'nearby_status=$?; sudo -n /opt/arkos-nearby/arkos-nearby launch-end "$nearby_launch_id"; exit "$nearby_status"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
nearby_launch_frontend() {
  if [ -n "$nearby_runtime" ]; then
    LD_LIBRARY_PATH="$nearby_runtime${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" /opt/retroarch/bin/"${emulator}" "$@"
  else
    /opt/retroarch/bin/"${emulator}" "$@"
  fi
}
