/ CV columns (tabular Track C): the mean of each span's rows (shots), per dimension. X is n x d quantized SigLIP rows
/ (i32 at 2^15), S holds k+1 row boundaries; thread i is (span, dimension). An empty span is 0.
s:i%d
j:i!d
a:word[S;s]
b:word[S;s+1]
v:sel[b>a;sum[t;b-a;word[X;((a+t)*d)+j]]%(b-a);0]
