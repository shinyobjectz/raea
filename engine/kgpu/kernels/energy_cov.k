/ Energy scans: the windowed covariance of two columns and their variances (Q8 inputs, 2r+1 rows), for the
/ picture-music relation; the host takes r = cov / sqrt(va vb). Thread i is (row, which): which 0 cov, 1 va, 2 vb.
row:i%3
k:i!3
w:(2*r)+1
ma:sum[j;w;shr[word[A;clamp[(row-r)+j;0;n-1]];8]]%w
mb:sum[j;w;shr[word[B;clamp[(row-r)+j;0;n-1]];8]]%w
v:sum[j;w;(sel[k=2;shr[word[B;clamp[(row-r)+j;0;n-1]];8]-mb;shr[word[A;clamp[(row-r)+j;0;n-1]];8]-ma])*sel[k=1;shr[word[A;clamp[(row-r)+j;0;n-1]];8]-ma;shr[word[B;clamp[(row-r)+j;0;n-1]];8]-mb]]%w
