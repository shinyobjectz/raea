import re,sys,json
t=open(sys.argv[1]).read()
gcs=re.findall(r"START \w (\d+)kb => (\w+),.*?=> (\d+)kb, p=([0-9.]+)ms \(max [0-9.]+\), cycle ([0-9.]+)ms END\]", t)
kinds={}
for before,kind,after,p,cyc in gcs:
    d=kinds.setdefault(kind,[0,0.0,0,[]]); d[0]+=1; d[1]+=float(p); d[2]+=int(before)-int(after); d[3].append(float(p))
j=json.load(open(sys.argv[2])); frames=j['presented']
for k,(n,ms,freed,l) in kinds.items():
    l.sort(); print(f"{k}: {n} collections, pause total {ms:.1f} ms, p50 {l[len(l)//2]:.2f} ms, max {l[-1]:.2f} ms, freed {freed/1024:.1f} MB")
tot=sum(v[1] for v in kinds.values()); fr=sum(v[2] for v in kinds.values())
print(f"frames {frames}; GC pause total {tot:.1f} ms = {tot/frames:.3f} ms/frame; garbage freed {fr/frames:.0f} KB/frame")
print("work p50/p99/max", round(j['work_ms']['p50'],2), round(j['work_ms']['p99'],2), round(j['work_ms']['max'],1), "| js p50/p99", round(j['js_ms']['p50'],3), round(j['js_ms']['p99'],3), "| missed", j['missed_vs_display'])
