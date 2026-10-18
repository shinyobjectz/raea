/ CV columns: each delta row's squared norm, on deltas shifted down 7 bits so the squares stay in 32 bits (the
/ caller takes the square root and scales by 2^7 / 2^15). Thread i is a row.
v:sum[j;d;(shr[word[D;(i*d)+j];7])*shr[word[D;(i*d)+j];7]]
