#!/bin/bash
# Download every manifest entry into its own empty folder under $1 (untrusted data; nothing in it is executed).
set -u
DL=$1; M=$(dirname "$0")/manifest.json
python3 -I -c "
import json,sys
for a in json.load(open(sys.argv[1])):
  url = a.get('url') or f\"https://zenodo.org/records/{a['zenodo']}/files/{a['uid']}.glb?download=1\"
  ext = 'stl' if a.get('smk') else 'glb'
  print(a['name'], url, ext)
" "$M" | while read name url ext; do
  mkdir -p "$DL/$name"; f="$DL/$name/source.$ext"
  if [ -s "$f" ]; then echo "have $name"; continue; fi
  curl -sSfL --retry 3 -o "$f.part" "$url" && mv "$f.part" "$f" && echo "got $name $(du -h "$f" | cut -f1)" || echo "FAILED $name"
done
