/ CV columns: 16-row windows every 8 rows, each row permuted by its place in the window (its words rotated by that
/ many), bundled by majority (more than 8 of 16 set). B holds the rows' 256-word hypervectors; thread i is (window,
/ word).
w:i%256
wd:i!256
v:sum[b;32;shl[(sum[r;16;and[shr[word[B;(((w*8)+r)*256)+((wd+256)-r)!256];b];1]])>8;b]]
