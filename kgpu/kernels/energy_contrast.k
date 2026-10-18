/ Energy scans: local contrast, the windowed variance over the windowed mean (2r+1 rows, edges clamped), on the column
/ shifted to Q8 so the squares stay in 32 bits; the answer is Q8 (variance Q16 over mean Q8 + 1).
w:(2*r)+1
m:sum[j;w;shr[word[C;clamp[(i-r)+j;0;n-1]];8]]%w
q:sum[j;w;(shr[word[C;clamp[(i-r)+j;0;n-1]];8]-m)*(shr[word[C;clamp[(i-r)+j;0;n-1]];8]-m)]%w
v:(q*256)%(m+1)
