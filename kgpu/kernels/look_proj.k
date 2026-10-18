/ Look hypervectors: thread i is (frame, word) of 64 words (2048 bits) per frame. Bit b of the frame is the sign of
/ the 432 features (G, from look_grid.k) weighted +1/-1 by a seeded integer hash of (b, feature): random hyperplanes
/ with no weight buffer, the same on every device.
/ Luma is centred on the frame's mean cell (the sum of its 144 luma cells over 144, truncated), so a look is its layout
/ of light rather than its brightness; previz.rs look_words is this function on the CPU, checked bit-equal.
f:i%64
wd:i!64
g:f*432
m:sum[c;144;word[G;g+3*c]]%144
v:sum[b;32;shl[(sum[k;432;(word[G;g+k]-sel[(k!3)=0;m;0])*(2*and[shr[(xor[(((wd*32)+b)*73856093);k*19349663]+40503)*-1640531535;15];1])-1])>0;b]]
