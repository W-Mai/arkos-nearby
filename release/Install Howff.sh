#!/bin/sh
set -eu
task_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
sudo -n mkdir -p /opt/howff-installer
sudo -n cp -- "$task_dir/howff" /opt/howff-installer/howff
sudo -n chmod 755 /opt/howff-installer/howff
exec sudo -n /opt/howff-installer/howff install --interactive
