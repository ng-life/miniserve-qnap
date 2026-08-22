#!/bin/sh
# Sourced by QDK from its temporary data-package directory. Run qbuild under
# fakeroot so these numeric IDs are recorded in data.tar without requiring a
# privileged build. QTS maps UID:GID 0:0 to admin:administrators.

find . -type d -exec chmod 0755 {} \; || return 1
find . -type f -exec chmod 0755 {} \; || return 1
chown -R 0:0 . || return 1
