/ CV columns: each row signed onto 8192 seeded integer-hash hyperplanes (+1/-1 per dimension, as look_proj.k),
/ 256 words a row. Thread i is (row, word).
r:i%256
wd:i!256
v:sum[b;32;shl[(sum[k;d;word[X;(r*d)+k]*(2*and[shr[(xor[(((wd*32)+b)*83492791);k*1103515245]+7919)*-1640531535;15];1])-1])>0;b]]
