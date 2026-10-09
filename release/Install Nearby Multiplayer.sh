#!/bin/sh
set -eu
task_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
sudo -n mkdir -p /opt/arkos-nearby-installer
sudo -n cp -- "$task_dir/arkos-nearby" /opt/arkos-nearby-installer/arkos-nearby
sudo -n chmod 755 /opt/arkos-nearby-installer/arkos-nearby
exec sudo -n /opt/arkos-nearby-installer/arkos-nearby install --interactive
