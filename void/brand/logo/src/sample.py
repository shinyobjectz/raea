import sys,json
W=H=500
d=open(sys.argv[1],'rb').read()
p=500/58; ox=1.0; oy=1.8
N=58
G=[]; amb=0
for j in range(N):
    row=[]
    for i in range(N):
        x0=ox+i*p; y0=oy+j*p
        vals=[d[int(y)*W+int(x)] for y in [y0+p*f for f in (0.25,0.4,0.5,0.6,0.75)] for x in [x0+p*f for f in (0.25,0.4,0.5,0.6,0.75)] if 0<=x<W and 0<=y<H]
        if not vals: row.append(0); continue
        dark=sum(v<128 for v in vals)/len(vals)
        if 0.2<dark<0.8: amb+=1
        row.append(1 if dark>=0.5 else 0)
    G.append(row)
rs=[j for j in range(N) if any(G[j])]; cs=[i for i in range(N) if any(G[j][i] for j in range(N))]
G=[r[cs[0]:cs[-1]+1] for r in G[rs[0]:rs[-1]+1]]
print('ambiguous',amb,'size',len(G[0]),'x',len(G))
for r in G: print(''.join('#' if c else '.' for c in r))
json.dump(G,open(sys.argv[2],'w'))
