"""Inspect Zenodo 3DBigDataSpace (Objaverse/Sketchfab mirror) records: Zenodo licence, the GLB (named by Sketchfab UID),
and the live Sketchfab metadata for that UID (licence, author, faces). Downloads thumb0 for a visual check.
usage: python3 -I inspect_zenodo.py OUT.json THUMBDIR id [id...]"""
import json, sys, time, urllib.request, os
out, thumbs, ids = sys.argv[1], sys.argv[2], sys.argv[3:]
def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "raea-assets/1.0"})
    with urllib.request.urlopen(req, timeout=60) as r: return r.read()
res = json.load(open(out)) if os.path.exists(out) else {}
for i in ids:
    if i in res: continue
    try:
        d = json.loads(get(f"https://zenodo.org/api/records/{i}"))
        m = d["metadata"]
        files = {f["key"]: f["size"] for f in d.get("files", [])}
        glbs = [k for k in files if k.endswith(".glb")]
        uid = glbs[0][:-4] if glbs else None
        row = dict(id=i, title=m["title"], zen_lic=(m.get("license") or {}).get("id"),
                   creators=[c.get("name") for c in m.get("creators", [])], glb=glbs[0] if glbs else None,
                   glb_size=files.get(glbs[0]) if glbs else None, uid=uid, desc=m.get("description", "")[:400])
        if uid:
            try:
                s = json.loads(get(f"https://api.sketchfab.com/v3/models/{uid}"))
                row.update(sf_name=s.get("name"), sf_lic=(s.get("license") or {}).get("label"),
                           sf_lic_slug=(s.get("license") or {}).get("slug"),
                           sf_user=s.get("user", {}).get("displayName"), sf_username=s.get("user", {}).get("username"),
                           sf_faces=s.get("faceCount"), sf_url=s.get("viewerUrl"))
            except Exception as e:
                row["sf_err"] = str(e)
        th = [k for k in files if k.startswith("thumb0")]
        if th:
            open(os.path.join(thumbs, f"{i}.jpg"), "wb").write(get(f"https://zenodo.org/records/{i}/files/{th[0]}?download=1"))
        res[i] = row
        print(i, row.get("title"), row.get("zen_lic"), row.get("sf_lic"), row.get("sf_faces"), round((row.get("glb_size") or 0)/1e6,1), "MB", flush=True)
    except Exception as e:
        print(i, "ERR", e, flush=True)
    json.dump(res, open(out, "w"), indent=1)
    time.sleep(0.5)
