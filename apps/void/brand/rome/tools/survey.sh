#!/bin/bash
DL=$1
for d in "$DL"/*/; do f=$(ls "$d"source.* | head -1)
  blender -b --factory-startup -P "$(dirname "$0")/convert.py" -- "$f" /dev/null --survey 2>&1 | grep '^SURVEY' | sed "s|^SURVEY|$(basename $d)|"
done
