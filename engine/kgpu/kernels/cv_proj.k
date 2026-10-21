/ CV columns: each row through the seeded projection P (d x 64, N(0, 1/64) at 2^13): X (2^15) . P -> n x 64 at 2^28.
/ Thread i is (row, column).
r:i%64
j:i!64
v:sum[k;d;word[X;(r*d)+k]*word[P;(k*64)+j]]
