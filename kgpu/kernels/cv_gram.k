/ CV columns: each row against each label (T, m x d, both at 2^15): the label gram, n x m at 2^30. Thread i is
/ (row, label).
r:i%m
l:i!m
v:sum[k;d;word[X;(r*d)+k]*word[T;(l*d)+k]]
